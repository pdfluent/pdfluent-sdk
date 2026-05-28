#!/usr/bin/env python3
"""XFA-B-005 — Measurement-artifact + semantics regression guards.

Stdlib + (optional) pikepdf. Pins the known false-positive classes so the batch
comparison can never regress into them. Run in gates:
  python3 scripts/xfa_fidelity/test_batch_comparison_semantics.py
Exit 0 = all guards pass.
"""
import importlib.util
import io
import json
import os
import sys
import tempfile
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))


def _load(name):
    spec = importlib.util.spec_from_file_location(name, os.path.join(HERE, name + ".py"))
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


FAILS = []


def check(cond, msg):
    if not cond:
        FAILS.append(msg)


def guard_rawbyte_not_xfa_retained():
    """A structurally-clean PDF with coincidental b'/XFA' bytes in a compressed
    stream must NOT be flagged as XFA retained."""
    try:
        import pikepdf
    except ImportError:
        print("  (skip rawbyte guard: pikepdf unavailable)")
        return
    cmp = _load("compare_pdfluent_to_oracle")
    with tempfile.TemporaryDirectory() as d:
        p = os.path.join(d, "clean.pdf")
        pdf = pikepdf.new()
        pdf.add_blank_page(page_size=(200, 200))
        # stream whose DECOMPRESSED content contains no /XFA, but raw bytes do
        pdf.make_stream(zlib.compress(b"q Q % /XFA appears only after decompression-coincidence"))
        pdf.save(p)
        with open(p, "ab") as fh:
            fh.write(b"\n%% coincidental /XFA bytes in trailer comment\n")
        raw = open(p, "rb").read()
        check(b"/XFA" in raw, "fixture must contain raw /XFA bytes")
        xfa, acro = cmp.structural_retention(p)
        check(xfa is False, "structural_retention flagged a clean PDF as XFA-retained (raw-byte false positive)")


def guard_real_xfa_detected():
    try:
        import pikepdf
    except ImportError:
        return
    cmp = _load("compare_pdfluent_to_oracle")
    with tempfile.TemporaryDirectory() as d:
        p = os.path.join(d, "xfa.pdf")
        pdf = pikepdf.new()
        pdf.add_blank_page(page_size=(200, 200))
        packet = pdf.make_stream(zlib.compress(b"<xdp:xdp></xdp:xdp>"))
        pdf.Root.AcroForm = pdf.make_indirect(pikepdf.Dictionary(XFA=pikepdf.Array([pikepdf.String("t"), packet])))
        pdf.save(p)
        xfa, acro = cmp.structural_retention(p)
        check(xfa is True, "structural_retention failed to detect a real AcroForm /XFA")


def guard_first_n_no_full_doc_claim():
    """A first_n record must not produce a full-doc page-count verdict; the
    validator must reject it if it does."""
    val = _load("validate_batch_comparison_results")
    base = {
        "oracle_record_id": "x::pdfrest", "doc_id": "x", "provider": "pdfrest",
        "oracle_scope": "first_n_pages", "first_n_pages": 3,
        "page_count_reliability": "first_n_only", "text_comparison_label": "first_n_text_only",
        "structural_xfa_retained": False, "reliability_class": "first-N-only",
        "private_artifact_policy": "report_only_no_commit",
        "page_count_verdict": "not_applicable_first_n", "defect_preclassification": ["first_n_inconclusive"],
    }
    # good record validates
    with tempfile.TemporaryDirectory() as d:
        gp = os.path.join(d, "good.json")
        json.dump({"schema_version": "1.0", "results": [base]}, open(gp, "w"))
        check(val.validate(gp) == 0, "valid first-N record wrongly rejected")
        # bad: first-N asserting a full-doc match
        bad = dict(base); bad["page_count_verdict"] = "match"
        bp = os.path.join(d, "bad.json")
        json.dump({"schema_version": "1.0", "results": [bad]}, open(bp, "w"))
        check(val.validate(bp) == 1, "validator failed to reject full-doc claim from first-N record")


def guard_no_artifact_paths_in_summary():
    val = _load("validate_batch_comparison_results")
    with tempfile.TemporaryDirectory() as d:
        rec = {
            "oracle_record_id": "y::pdfrest", "doc_id": "y", "provider": "pdfrest",
            "oracle_scope": "full_document", "first_n_pages": None,
            "page_count_reliability": "reliable", "text_comparison_label": "full_text",
            "structural_xfa_retained": False, "reliability_class": "reliable",
            "private_artifact_policy": "report_only_no_commit", "page_count_verdict": "match",
            "defect_preclassification": ["ok_or_negligible"], "notes": "see /opt/x/out.pdf",
        }
        bp = os.path.join(d, "bad.json")
        json.dump({"schema_version": "1.0", "results": [rec]}, open(bp, "w"))
        check(val.validate(bp) == 1, "validator failed to reject embedded .pdf path in summary")


def guard_missing_png_not_visual_verdict():
    """Missing oracle PNGs must leave eligible_for_milestone_c handling to the
    comparison runner (not a pass/fail) — preclassify must not emit a visual verdict."""
    cmp = _load("compare_pdfluent_to_oracle")
    rec = {
        "page_count_reliability": "reliable", "page_count_oracle": 4, "page_count_pdfluent": 4,
        "text_ratio": 0.99, "oracle_scope": "full_document", "structural_xfa_retained": False,
        "file_size_ratio": 1.1, "error": None,
    }
    pre = cmp.preclassify(rec)
    check(all("visual" not in p for p in pre), "preclassify emitted a visual verdict (visual belongs to Milestone C)")


def main():
    for fn in [guard_rawbyte_not_xfa_retained, guard_real_xfa_detected,
               guard_first_n_no_full_doc_claim, guard_no_artifact_paths_in_summary,
               guard_missing_png_not_visual_verdict]:
        fn()
    if FAILS:
        print(f"FAIL: {len(FAILS)} guard(s)")
        for f in FAILS:
            print("  -", f)
        return 1
    print("OK: all measurement-artifact + semantics guards pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
