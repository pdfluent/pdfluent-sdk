#!/usr/bin/env python3
"""Read-only validator for an XFA oracle truth table.

Dependency-light (stdlib only). Checks each record against the field contract in
docs/xfa/fidelity/oracle_truth_table.schema.json plus the consistency rules from
XFA_ORACLE_TRUTH_TABLE_SPEC.md. Exits non-zero on any violation. Never mutates.

Usage: python3 scripts/xfa_fidelity/validate_oracle_truth_table.py <truth_table.json>
"""
import json, re, sys

REQUIRED = [
    "doc_id","source_pdf_path","source_sha256","oracle_flat_pdf_path","oracle_sha256",
    "oracle_source","coverage","coverage_n","oracle_pngs_available","oracle_text_available",
    "page_count_reliable","sensitivity","can_render_locally","can_commit_derived_artifacts",
    "known_defectclass_labels","protected_target_status","prior_wave_refs",
]
ORACLE_SRC = {"adobe","pdfrest","speedtest","manual_official","customer"}
COVERAGE = {"full_document","first_n_pages"}
SENS = {"public","corpus-private","operator-private","customer-private"}
PROT = {"none","protected","regression-sensitive"}
SHA = re.compile(r"^[a-f0-9]{64}$")
PRIVATE = {"operator-private","customer-private"}


def validate(path):
    doc = json.load(open(path))
    errs = []
    if doc.get("schema_version") != "1.0":
        errs.append("schema_version must be '1.0'")
    recs = doc.get("records", [])
    seen = set()
    for i, r in enumerate(recs):
        tag = r.get("doc_id", f"#{i}")
        for f in REQUIRED:
            if f not in r:
                errs.append(f"{tag}: missing field '{f}'")
        if r.get("doc_id") in seen:
            errs.append(f"{tag}: duplicate doc_id")
        seen.add(r.get("doc_id"))
        if r.get("oracle_source") not in ORACLE_SRC:
            errs.append(f"{tag}: bad oracle_source {r.get('oracle_source')!r}")
        if r.get("coverage") not in COVERAGE:
            errs.append(f"{tag}: bad coverage {r.get('coverage')!r}")
        if r.get("sensitivity") not in SENS:
            errs.append(f"{tag}: bad sensitivity {r.get('sensitivity')!r}")
        if r.get("protected_target_status") not in PROT:
            errs.append(f"{tag}: bad protected_target_status")
        for shf in ("source_sha256","oracle_sha256"):
            v = r.get(shf)
            if v is not None and not SHA.match(str(v)):
                errs.append(f"{tag}: {shf} not a sha256")
        # consistency rules (XFA_ORACLE_TRUTH_TABLE_SPEC.md)
        if r.get("coverage") == "first_n_pages":
            if not isinstance(r.get("coverage_n"), int) or r.get("coverage_n") < 1:
                errs.append(f"{tag}: first_n_pages requires coverage_n>=1")
            if r.get("page_count_reliable") is not False:
                errs.append(f"{tag}: first_n_pages must have page_count_reliable=false")
        if r.get("coverage") == "full_document" and r.get("coverage_n") is not None:
            errs.append(f"{tag}: full_document must have coverage_n=null")
        if r.get("sensitivity") in PRIVATE and r.get("can_commit_derived_artifacts") is not False:
            errs.append(f"{tag}: private doc must have can_commit_derived_artifacts=false")
        if r.get("oracle_flat_pdf_path") in (None, "") and r.get("page_count_reliable"):
            errs.append(f"{tag}: page_count_reliable=true but no oracle_flat_pdf_path")
    if errs:
        print(f"FAIL: {len(errs)} violation(s) in {len(recs)} records")
        for e in errs[:50]:
            print("  -", e)
        return 1
    print(f"OK: {len(recs)} records valid")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("usage: validate_oracle_truth_table.py <truth_table.json>"); sys.exit(2)
    sys.exit(validate(sys.argv[1]))
