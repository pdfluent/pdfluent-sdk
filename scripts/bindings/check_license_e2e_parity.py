#!/usr/bin/env python3
"""Static checker for the commercial-license E2E parity matrix.

Verifies, read-only:

1. The matrix at
   `benchmarks/runs/ga_100_closure_v3/commercial_license_e2e/
   LICENSE_E2E_MATRIX.json` is parseable and uses only allowed
   classifications.
2. Every `supported` cell has a non-empty `evidence` pointer; every path
   token in that pointer resolves to a file under `crates/` or
   `bindings/` where applicable.
3. Every `intentionally_unsupported` cell has a non-empty `rationale`.
4. Every `beta_limitation` cell carries a precise follow-up.
5. There are zero `bug` and zero `missing` cells (domain closure
   precondition).
6. The "no silent fallback after license failure" rule (matrix flow
   `F13_no_silent_fallback_after_license_failure`) is `supported` for
   every binding — this is a hard release-gate invariant.
7. The "typed code" rule (matrix flow `F10_typed_error_code_field`) is
   `supported` for every binding.
8. For every binding column that the matrix claims `supported` on
   F03 / F04 (invalid + unknown-tier typed error), there is at least one
   test-file path in the evidence pointer.
9. The error_catalogue.md table mentions every supported binding for the
   three `E-LICENSE-*` codes (no stale "gap" annotation for codes that
   are actually closed). Coarse grep; positive findings raise warnings.

Exit codes:
  0 → matrix internally consistent + closure preconditions met.
  1 → structural defect (allowed values, missing rationale, etc.).
  2 → `bug` or `missing` cell present, or silent-fallback rule violated.
  3 → typed-code rule violated.
  4 → catalogue drift (`--strict-catalogue` upgrades to failure).

`--json` for machine-readable output.
`--verbose` for per-cell diagnostics.
`--strict-catalogue` to fail when a license code is still annotated
  "(gap)" in `docs/error_catalogue.md` for a binding the matrix marks
  `supported`.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
MATRIX_PATH = (
    REPO_ROOT
    / "benchmarks"
    / "runs"
    / "ga_100_closure_v3"
    / "commercial_license_e2e"
    / "LICENSE_E2E_MATRIX.json"
)
CATALOGUE_PATH = REPO_ROOT / "docs" / "error_catalogue.md"

ALLOWED_STATUSES = {
    "supported",
    "intentionally_unsupported",
    "beta_limitation",
    "missing",
    "bug",
}

# Map binding id → catalogue column header (used for the catalogue cross-
# check in step 9). The columns appear in this order in the catalogue's
# license rows: Python | WASM/TS | Node | .NET | Java | C ABI.
BINDING_TO_CATALOGUE_COLUMN_INDEX = {
    "python": 0,
    "wasm": 1,
    "node": 2,
    "dotnet": 3,
    "java": 4,
    "cabi": 5,
}

LICENSE_CODES = [
    "E-LICENSE-INVALID",
    "E-LICENSE-FEATURE-NOT-IN-TIER",
    "E-LICENSE-CAPABILITY-NOT-COMPILED",
]


def _load(path: Path) -> dict[str, Any]:
    if not path.exists():
        sys.exit(f"matrix not found at {path}")
    return json.loads(path.read_text(encoding="utf-8"))


def _evidence_paths_resolve(evidence: str) -> tuple[bool, list[str]]:
    """Returns (resolved, missing_paths).

    The evidence string contains both file-path tokens (recognised by
    starting with a known top-level directory like `crates/` or
    `bindings/`) and free-text parentheticals that may contain slashes
    (e.g. `16/17/18` or `inactive/trial`). Only tokens that look like
    real source paths are checked.

    Resolved if at least one real path token exists AND every recognised
    real path token resolves. Pure symbol-name evidence (no path tokens)
    is accepted.
    """
    missing: list[str] = []
    resolved: list[str] = []
    path_prefixes = ("crates/", "bindings/", "scripts/", "docs/", "pdfluent-examples/", "benchmarks/")
    for tok in re.split(r"[;,\s]+", evidence):
        tok = tok.strip().strip("()`'\".,;:")
        if not tok or not tok.startswith(path_prefixes):
            continue
        candidate = REPO_ROOT / tok.split(":")[0].split("#")[0]
        if candidate.exists():
            resolved.append(tok)
        else:
            missing.append(tok)
    if missing:
        return False, missing
    return True, []


def check(verbose: bool = False, strict_catalogue: bool = False) -> tuple[int, dict[str, Any]]:
    matrix = _load(MATRIX_PATH)
    issues: list[dict[str, Any]] = []
    counts = {s: 0 for s in ALLOWED_STATUSES}
    per_flow_counts: dict[str, dict[str, int]] = {}

    flows = matrix["flows"]
    bindings = matrix["bindings"]

    # 1-5: structural + closure checks
    for flow in flows:
        per_flow_counts.setdefault(flow["id"], {s: 0 for s in ALLOWED_STATUSES})
        for binding, cell in flow["per_binding"].items():
            status = cell.get("status")
            if status not in ALLOWED_STATUSES:
                issues.append(
                    {"severity": "error", "kind": "invalid_status",
                     "flow": flow["id"], "binding": binding, "status": status}
                )
                continue
            counts[status] += 1
            per_flow_counts[flow["id"]][status] += 1

            if status == "supported":
                evidence = cell.get("evidence", "")
                if not evidence:
                    issues.append(
                        {"severity": "error", "kind": "missing_evidence",
                         "flow": flow["id"], "binding": binding}
                    )
                else:
                    ok, missing = _evidence_paths_resolve(evidence)
                    if not ok:
                        issues.append(
                            {"severity": "warning", "kind": "unresolved_evidence_path",
                             "flow": flow["id"], "binding": binding,
                             "missing_paths": missing}
                        )
            elif status == "intentionally_unsupported":
                if not cell.get("rationale"):
                    issues.append(
                        {"severity": "error", "kind": "missing_rationale",
                         "flow": flow["id"], "binding": binding}
                    )
            elif status == "beta_limitation":
                if not (cell.get("followup") or cell.get("rationale")):
                    issues.append(
                        {"severity": "error", "kind": "missing_followup",
                         "flow": flow["id"], "binding": binding}
                    )

    # 6: hard rule — no silent fallback after license failure
    silent_fallback_violations = []
    for flow in flows:
        if flow["id"] == "F13_no_silent_fallback_after_license_failure":
            for binding, cell in flow["per_binding"].items():
                if cell.get("status") != "supported":
                    silent_fallback_violations.append(
                        {"binding": binding, "status": cell.get("status")}
                    )

    # 7: typed-code rule — F10 must be supported across all bindings
    typed_code_violations = []
    for flow in flows:
        if flow["id"] == "F10_typed_error_code_field":
            for binding, cell in flow["per_binding"].items():
                if cell.get("status") != "supported":
                    typed_code_violations.append(
                        {"binding": binding, "status": cell.get("status")}
                    )

    # 7b: signed-payload rule — F15 (expired), F18 (valid signed payload),
    # F19 (tampered signature) MUST be supported on every binding.
    # If any of these flows is missing from the matrix, that is itself
    # a violation (the matrix is incomplete).
    signed_payload_violations: list[dict[str, Any]] = []
    REQUIRED_SIGNED_FLOWS = (
        "F15_expired_license_typed",
        "F18_signed_payload_valid_activates_tier",
        "F19_signed_payload_tampered_signature_typed",
    )
    flow_by_id = {f["id"]: f for f in flows}
    for required_flow in REQUIRED_SIGNED_FLOWS:
        flow = flow_by_id.get(required_flow)
        if flow is None:
            signed_payload_violations.append(
                {
                    "flow": required_flow,
                    "binding": "*",
                    "kind": "flow_missing_from_matrix",
                }
            )
            continue
        for binding, cell in flow["per_binding"].items():
            if cell.get("status") != "supported":
                signed_payload_violations.append(
                    {
                        "flow": required_flow,
                        "binding": binding,
                        "kind": "non_supported_cell",
                        "status": cell.get("status"),
                    }
                )

    # 8: F03 / F04 evidence must include a test-file path token
    for flow_id in ("F03_no_silent_fallback_on_invalid", "F04_unknown_tier_typed_error"):
        flow = next((f for f in flows if f["id"] == flow_id), None)
        if not flow:
            continue
        for binding, cell in flow["per_binding"].items():
            if cell.get("status") != "supported":
                continue
            ev = cell.get("evidence", "")
            has_test_path = any(
                tok.strip().strip("()`'\"") and "/" in tok and (
                    "tests/" in tok or "test/" in tok or "/test_" in tok
                )
                for tok in re.split(r"[;,\s]+", ev)
            )
            if not has_test_path:
                issues.append(
                    {"severity": "warning", "kind": "no_test_evidence",
                     "flow": flow_id, "binding": binding,
                     "evidence_snippet": ev[:120]}
                )

    # 9: catalogue cross-check — license rows in error_catalogue.md must
    # NOT still annotate "(gap)" for bindings the matrix says supported.
    catalogue_warnings = []
    if CATALOGUE_PATH.exists():
        cat_text = CATALOGUE_PATH.read_text(encoding="utf-8")
        for code in LICENSE_CODES:
            # Match the markdown row that starts with `| `<code>``.
            row_re = re.compile(
                r"^\|\s*`" + re.escape(code) + r"`\s*\|.*$",
                re.MULTILINE,
            )
            m = row_re.search(cat_text)
            if not m:
                continue
            cells = [c.strip() for c in m.group(0).split("|")]
            # Layout: '' | code | variant | description | how-to-fix |
            # py | wasm | node | dotnet | java | cabi | ''
            try:
                py, wasm, node, dotnet, java, cabi = cells[5:11]
            except ValueError:
                continue
            cat_columns = {
                "python": py, "wasm": wasm, "node": node,
                "dotnet": dotnet, "java": java, "cabi": cabi,
            }
            # The F10 flow asserts typed-code parity per binding. If
            # F10/<binding> is supported and the catalogue still says
            # "(gap)" / "TBD", that is documentation drift.
            for binding, col in cat_columns.items():
                if "(gap)" not in col and "TBD" not in col:
                    continue
                # Special case: cabi for FEATURE-NOT-IN-TIER and
                # CAPABILITY-NOT-COMPILED is a real ongoing gap (still
                # ErrorUnknown). Accept "(gap)" annotation there.
                if binding == "cabi" and code != "E-LICENSE-INVALID":
                    continue
                catalogue_warnings.append(
                    {"severity": "warning", "kind": "catalogue_stale_gap",
                     "code": code, "binding": binding,
                     "catalogue_cell": col[:80]}
                )

    # Verdict
    closure_violation = (counts.get("bug", 0) > 0
                        or counts.get("missing", 0) > 0)
    structural_errors = [i for i in issues if i["severity"] == "error"]
    rc = 0
    if structural_errors:
        rc = 1
    if closure_violation or silent_fallback_violations:
        rc = max(rc, 2)
    if typed_code_violations:
        rc = max(rc, 3)
    if signed_payload_violations:
        rc = max(rc, 5)
    if strict_catalogue and catalogue_warnings:
        rc = max(rc, 4)

    verdict = (
        "COMMERCIAL_LICENSE_E2E_TRUE_100_PERCENT_GREEN"
        if rc == 0
        and not silent_fallback_violations
        and not typed_code_violations
        and not signed_payload_violations
        else "COMMERCIAL_LICENSE_E2E_BLOCKED"
    )

    result = {
        "matrix_path": str(MATRIX_PATH.relative_to(REPO_ROOT)),
        "total_flows": len(flows),
        "total_bindings": len(bindings),
        "total_cells": sum(counts.values()),
        "per_status": counts,
        "structural_issues": issues,
        "silent_fallback_violations": silent_fallback_violations,
        "typed_code_violations": typed_code_violations,
        "signed_payload_violations": signed_payload_violations,
        "catalogue_warnings": catalogue_warnings,
        "closure_violation": closure_violation,
        "verdict": verdict,
    }

    if verbose:
        print(f"matrix: {result['matrix_path']}")
        print(
            f"cells: {result['total_cells']} "
            f"(flows={result['total_flows']} bindings={result['total_bindings']})"
        )
        for s, c in counts.items():
            print(f"  {s:30} = {c}")
        if issues:
            print(f"\nstructural issues ({len(issues)}):")
            for i in issues[:20]:
                print(f"  [{i['severity']}] {i['kind']}: {i.get('flow','')} / {i.get('binding','')}")
        if silent_fallback_violations:
            print("\nsilent-fallback violations:")
            for v in silent_fallback_violations:
                print(f"  binding={v['binding']} status={v['status']}")
        if typed_code_violations:
            print("\ntyped-code violations:")
            for v in typed_code_violations:
                print(f"  binding={v['binding']} status={v['status']}")
        if signed_payload_violations:
            print("\nsigned-payload violations:")
            for v in signed_payload_violations:
                print(
                    f"  flow={v['flow']} binding={v['binding']} "
                    f"kind={v['kind']} status={v.get('status', 'n/a')}"
                )
        if catalogue_warnings:
            print(f"\ncatalogue warnings ({len(catalogue_warnings)}):")
            for w in catalogue_warnings:
                print(
                    f"  {w['code']}/{w['binding']} still annotated: "
                    f"{w['catalogue_cell']!r}"
                )

    return rc, result


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0] if __doc__ else "")
    p.add_argument("--json", action="store_true")
    p.add_argument("--verbose", action="store_true")
    p.add_argument("--strict-catalogue", action="store_true")
    args = p.parse_args(argv)
    rc, result = check(verbose=args.verbose, strict_catalogue=args.strict_catalogue)
    if args.json:
        json.dump(result, sys.stdout, indent=2)
        sys.stdout.write("\n")
    else:
        s = result["per_status"]
        print(
            f"license-e2e-parity: total={result['total_cells']} "
            f"supported={s.get('supported',0)} "
            f"intentionally_unsupported={s.get('intentionally_unsupported',0)} "
            f"beta_limitation={s.get('beta_limitation',0)} "
            f"bug={s.get('bug',0)} missing={s.get('missing',0)}"
        )
        print(f"verdict: {result['verdict']}")
        if result["structural_issues"]:
            errs = sum(1 for i in result["structural_issues"] if i["severity"] == "error")
            warns = sum(1 for i in result["structural_issues"] if i["severity"] == "warning")
            print(f"structural issues: {errs} errors, {warns} warnings (rerun with --verbose)")
        if result["silent_fallback_violations"]:
            print(
                f"silent-fallback violations: {len(result['silent_fallback_violations'])} "
                "(CRITICAL — hard release-gate invariant)"
            )
        if result["typed_code_violations"]:
            print(
                f"typed-code violations: {len(result['typed_code_violations'])} "
                "(blocks closure)"
            )
        if result.get("signed_payload_violations"):
            print(
                f"signed-payload violations: {len(result['signed_payload_violations'])} "
                "(blocks TRUE 100% closure — F15/F18/F19 must be supported per binding)"
            )
        if result["catalogue_warnings"]:
            print(
                f"catalogue warnings: {len(result['catalogue_warnings'])}"
                + (" (failing under --strict-catalogue)" if args.strict_catalogue else "")
            )
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
