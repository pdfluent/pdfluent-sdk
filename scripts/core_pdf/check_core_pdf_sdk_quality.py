#!/usr/bin/env python3
"""Domain gate for CORE_PDF_SDK_ENTERPRISE_QUALITY_100.

Validates that the non-XFA core PDF SDK quality domain is complete:
  * capability matrix is structurally valid
  * no `bug` / `missing` / `beta_limitation` cells anywhere in the matrix
  * every `supported` cell has an evidence pointer
  * every `intentionally_unsupported` / `not_exposed_by_design` cell has a rationale
  * fixture manifest exists
  * performance baseline JSON exists
  * security hardening report exists

Exit non-zero if the domain is not closed.

Usage:
  python3 scripts/core_pdf/check_core_pdf_sdk_quality.py
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
BASE = REPO / "benchmarks/runs/ga_100_closure_v3/core_pdf_sdk_quality"
MATRIX = BASE / "CORE_PDF_SDK_CAPABILITY_MATRIX.json"
FIXTURE_MANIFEST = BASE / "CORE_PDF_FIXTURE_MANIFEST.json"
PERF_BASELINE = BASE / "CORE_PDF_PERFORMANCE_BASELINE.json"
SECURITY_REPORT = BASE / "CORE_PDF_SECURITY_HARDENING_REPORT.md"

# Hardened for CORE_PDF_SDK_ENTERPRISE_TRUE_100: no soft/vague statuses.
FORBIDDEN = {
    "bug",
    "missing",
    "beta_limitation",
    "out_of_scope",
    # the pre-followup soft statuses are no longer acceptable as 100%-closure
    "not_exposed_by_design",
    "intentionally_unsupported",
}
# Non-supported statuses allowed ONLY with full enforcement
# (rationale + evidence + expected_developer_behavior).
NEEDS_RATIONALE = {
    "intentionally_unsupported_v1",
    "not_exposed_by_design_with_typed_error",
    "split_read_supported_write_unsupported",
}


def main() -> int:
    errors: list[str] = []

    if not MATRIX.exists():
        print(f"FAIL: matrix missing: {MATRIX}", file=sys.stderr)
        return 1
    matrix = json.loads(MATRIX.read_text())

    cells = list(matrix.get("rust_core", []))
    if not cells:
        errors.append("matrix.rust_core is empty")

    for c in cells:
        cap = c.get("capability", "<unnamed>")
        st = c.get("status")
        if st in FORBIDDEN:
            errors.append(f"forbidden status '{st}' on capability '{cap}' (must be fixed, not deferred)")
        elif st == "supported":
            if not c.get("evidence"):
                errors.append(f"supported capability '{cap}' has no evidence pointer")
        elif st in NEEDS_RATIONALE:
            # Hard enforcement: every non-supported cell must carry
            # rationale + evidence + expected developer behavior.
            if not c.get("rationale"):
                errors.append(f"capability '{cap}' status '{st}' needs a rationale")
            if not c.get("evidence"):
                errors.append(f"capability '{cap}' status '{st}' needs an evidence pointer")
            if not c.get("expected_developer_behavior"):
                errors.append(
                    f"capability '{cap}' status '{st}' needs 'expected_developer_behavior'"
                )
        else:
            errors.append(f"capability '{cap}' has unknown/disallowed status '{st}'")

    # binding smokes: each binding cell must be supported (or a hardened status)
    for b in matrix.get("bindings_non_xfa_smoke", {}).get("bindings", []):
        if b.get("status") not in ({"supported"} | NEEDS_RATIONALE):
            errors.append(f"binding '{b.get('binding')}' has invalid status '{b.get('status')}'")
        if b.get("status") == "supported" and not b.get("smoke"):
            errors.append(f"binding '{b.get('binding')}' supported but no smoke evidence")

    # tally must reflect zero forbidden
    tally = matrix.get("tally", {})
    for k in FORBIDDEN:
        if int(tally.get(k, 0)) != 0:
            errors.append(f"tally reports {tally.get(k)} '{k}' cells (must be 0)")

    # required companion artifacts
    for label, p in [
        ("fixture manifest", FIXTURE_MANIFEST),
        ("performance baseline", PERF_BASELINE),
        ("security hardening report", SECURITY_REPORT),
    ]:
        if not p.exists():
            errors.append(f"{label} missing: {p.relative_to(REPO)}")

    if errors:
        print("CORE_PDF_SDK_QUALITY: NOT CLOSED", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1

    sup = tally.get("supported", 0)
    nox = tally.get("not_exposed_by_design", 0) + tally.get("intentionally_unsupported", 0)
    print(
        f"CORE_PDF_SDK_QUALITY: OK — {sup} supported, {nox} explicit out-of-scope, "
        f"0 bug/missing/beta; fixtures + perf + security present."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
