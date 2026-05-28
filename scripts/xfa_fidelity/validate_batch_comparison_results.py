#!/usr/bin/env python3
"""XFA-B-010 — Read-only validator for batch comparison results.

Stdlib only. Fails (non-zero) on any of:
 - invalid schema / missing required fields;
 - full-document page-count claim from a first_n_pages record;
 - structural XFA flag that is not a structural boolean (no raw-byte counts);
 - missing reliability_class;
 - private artifact marked commit-safe incorrectly;
 - missing provenance (provider/oracle_scope);
 - defect finalization before Milestone C (a `final_defectclass` field is banned);
 - any committed JSON record referencing a PDF/PNG path (private artifacts must
   not be embedded in committed summaries).

Usage: validate_batch_comparison_results.py <batch_comparison_results.json>
"""
import json, re, sys

REQUIRED = [
    "oracle_record_id", "doc_id", "provider", "oracle_scope", "first_n_pages",
    "page_count_reliability", "text_comparison_label", "structural_xfa_retained",
    "reliability_class", "private_artifact_policy", "page_count_verdict",
    "defect_preclassification",
]
RELIABILITY = {"reliable", "first-N-only", "inconclusive", "measurement-artifact", "private-artifact-limited"}
PDFPNG = re.compile(r"\.(pdf|png)(\"|$)", re.I)
BANNED_FIELDS = {"final_defectclass", "defectclass_final", "confirmed_defectclass"}


def validate(path):
    doc = json.load(open(path))
    errs = []
    if doc.get("schema_version") != "1.0":
        errs.append("schema_version must be '1.0'")
    results = doc.get("results", [])
    for i, r in enumerate(results):
        tag = r.get("oracle_record_id", f"#{i}")
        for f in REQUIRED:
            if f not in r:
                errs.append(f"{tag}: missing '{f}'")
        for b in BANNED_FIELDS:
            if b in r:
                errs.append(f"{tag}: banned field '{b}' (no defect finalization before Milestone C)")
        if r.get("reliability_class") not in RELIABILITY:
            errs.append(f"{tag}: bad/missing reliability_class {r.get('reliability_class')!r}")
        if not r.get("provider") or not r.get("oracle_scope"):
            errs.append(f"{tag}: missing provenance (provider/oracle_scope)")
        # first-N must not assert full-doc page count
        if r.get("oracle_scope") == "first_n_pages":
            if r.get("page_count_verdict") != "not_applicable_first_n":
                errs.append(f"{tag}: first_n record asserts page_count_verdict={r.get('page_count_verdict')}")
            if r.get("page_count_reliability") != "first_n_only":
                errs.append(f"{tag}: first_n record reliability != first_n_only")
            if r.get("text_comparison_label") != "first_n_text_only":
                errs.append(f"{tag}: first_n record text label != first_n_text_only")
        # structural xfa flag must be boolean or null (not a raw count)
        xr = r.get("structural_xfa_retained", None)
        if xr is not None and not isinstance(xr, bool):
            errs.append(f"{tag}: structural_xfa_retained must be bool/null (raw-byte counts banned)")
        # private artifact must be report-only
        if r.get("private_artifact_policy") == "commit_ok" and r.get("reliability_class") == "private-artifact-limited":
            errs.append(f"{tag}: private-artifact-limited record marked commit_ok")
    # no PDF/PNG paths embedded in the committed summary
    blob = json.dumps(doc)
    for m in set(re.findall(r'"[^"]*\.(?:pdf|png)"', blob)):
        errs.append(f"committed JSON references artifact path {m} (private artifacts must not be embedded)")

    if errs:
        print(f"FAIL: {len(errs)} violation(s) in {len(results)} results")
        for e in errs[:60]:
            print("  -", e)
        return 1
    print(f"OK: {len(results)} results valid")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("usage: validate_batch_comparison_results.py <results.json>"); sys.exit(2)
    sys.exit(validate(sys.argv[1]))
