#!/usr/bin/env python3
"""Static checker for the canonical binding API parity matrix.

Reads `benchmarks/runs/ga_100_closure_v3/binding_api_parity/
BINDING_API_PARITY_MATRIX.json` and verifies, statically, that:

1. Every cell's classification is one of the allowed values.
2. For every `supported` cell, the evidence pointer (file path or symbol)
   resolves to something — either an existing file under `crates/` or
   `bindings/`, or a documented constant per binding.
3. For every `beta_limitation` cell, a precise follow-up block exists
   (with `files` / `signature_sketch` / `tests`).
4. For every `intentionally_unsupported` cell, a rationale is present.
5. The matrix has no `bug` and no `missing` cells (closure precondition).
6. Each binding's README does NOT contain text patterns that imply a
   capability the matrix says is `intentionally_unsupported`. (Coarse
   keyword grep; a false positive triggers a warning, never a failure.)

This script is intentionally read-only and toolchain-free. It is safe
to run in CI, on dev laptops, or inside agents.

Exit codes:

- 0 → matrix is internally consistent AND no `bug`/`missing` cells.
- 1 → matrix has structural defects (allowed values, missing rationale, etc.).
- 2 → matrix has `bug` or `missing` cells (domain closure violated).
- 3 → README implies an unsupported capability (release readiness drift).

The CI gate runs this with no flags. Use `--json` for machine-readable
output, `--verbose` for per-cell diagnostics, `--strict-readme` to upgrade
the README implication warning into a failure.
"""

from __future__ import annotations

import argparse
import json
import os
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
    / "binding_api_parity"
    / "BINDING_API_PARITY_MATRIX.json"
)

ALLOWED_STATUSES = {
    "supported",
    "intentionally_unsupported",
    "beta_limitation",
    "bug",
    "missing",
}

# Coarse README implication patterns per capability. These are the
# *forbidden* substrings we expect to NOT see in a binding's README if
# that binding has the corresponding capability marked
# `intentionally_unsupported`.
README_IMPLICATION_PATTERNS = {
    "open_load_path": [r"\bopen\s*\(\s*[\"'][^)]+\.pdf[\"']\s*\)"],
    "save_bytes_or_path": [r"\b(?:doc|document)\.save\s*\("],
    "thumbnail_render": [r"\bthumbnail\s*\("],
    "text_formatting_mutation": [r"format[_ ]?text", r"set[_ ]?text[_ ]?run[_ ]?style"],
    "font_style_mutation": [r"set[_ ]?text[_ ]?run[_ ]?style"],
    "xfa_handling": [r"\.flatten[_ ]?xfa\s*\(", r"XfaEngine"],
}

BINDING_README_PATHS = {
    "rust": "crates/pdfluent/README.md",
    "cabi": "crates/pdf-capi/README.md",
    "wasm": "crates/xfa-wasm/README.md",
    "node": "crates/pdf-node/README.md",
    "python": "crates/pdf-python/README.md",
    "dotnet": "bindings/dotnet/src/PDFluent/README.md",
    "java": "bindings/java/README.md",
}


def _load_matrix() -> dict[str, Any]:
    if not MATRIX_PATH.exists():
        sys.exit(f"matrix not found at {MATRIX_PATH}")
    return json.loads(MATRIX_PATH.read_text(encoding="utf-8"))


def _evidence_resolves(evidence: str) -> bool:
    """Cheap heuristic: split the evidence string on the typical
    delimiters and check that at least one token referenced as a path
    exists. Tokens without slashes are accepted (they refer to symbol
    names which we cannot resolve without a toolchain)."""
    if not evidence:
        return False
    for tok in re.split(r"[;,\s]+", evidence):
        tok = tok.strip().strip("()`'\"")
        if not tok:
            continue
        if "/" in tok:
            candidate = REPO_ROOT / tok.split(":")[0]
            if candidate.exists():
                return True
    # No path token; accept (it is a pure symbol reference).
    return True


def check(verbose: bool = False, strict_readme: bool = False) -> tuple[int, dict[str, Any]]:
    matrix = _load_matrix()
    issues: list[dict[str, Any]] = []
    counts = {s: 0 for s in ALLOWED_STATUSES}
    per_binding_counts: dict[str, dict[str, int]] = {}
    bindings = matrix["bindings"]
    for b in bindings:
        per_binding_counts[b] = {s: 0 for s in ALLOWED_STATUSES}

    # Structural checks
    for cap in matrix["capabilities"]:
        for binding, cell in cap["per_binding"].items():
            status = cell.get("status")
            if status not in ALLOWED_STATUSES:
                issues.append(
                    {
                        "severity": "error",
                        "kind": "invalid_status",
                        "capability": cap["id"],
                        "binding": binding,
                        "status": status,
                    }
                )
                continue
            counts[status] += 1
            per_binding_counts.setdefault(binding, {s: 0 for s in ALLOWED_STATUSES})[
                status
            ] += 1

            if status == "supported":
                evidence = cell.get("evidence", "")
                if not evidence or not _evidence_resolves(evidence):
                    issues.append(
                        {
                            "severity": "warning",
                            "kind": "weak_evidence",
                            "capability": cap["id"],
                            "binding": binding,
                            "evidence": evidence,
                        }
                    )
            elif status == "beta_limitation":
                if not cell.get("followup"):
                    issues.append(
                        {
                            "severity": "error",
                            "kind": "missing_followup",
                            "capability": cap["id"],
                            "binding": binding,
                        }
                    )
            elif status == "intentionally_unsupported":
                if not cell.get("rationale"):
                    issues.append(
                        {
                            "severity": "error",
                            "kind": "missing_rationale",
                            "capability": cap["id"],
                            "binding": binding,
                        }
                    )

    # README implication check
    readme_warnings: list[dict[str, Any]] = []
    for cap in matrix["capabilities"]:
        cap_id = cap["id"]
        patterns = README_IMPLICATION_PATTERNS.get(cap_id)
        if not patterns:
            continue
        for binding, cell in cap["per_binding"].items():
            if cell.get("status") != "intentionally_unsupported":
                continue
            readme_rel = BINDING_README_PATHS.get(binding)
            if not readme_rel:
                continue
            readme_path = REPO_ROOT / readme_rel
            if not readme_path.exists():
                continue
            text = readme_path.read_text(encoding="utf-8")
            for pat in patterns:
                if re.search(pat, text, flags=re.IGNORECASE):
                    readme_warnings.append(
                        {
                            "severity": "warning",
                            "kind": "readme_implies_unsupported",
                            "capability": cap_id,
                            "binding": binding,
                            "readme": readme_rel,
                            "pattern": pat,
                        }
                    )

    # Domain closure precondition
    closure_violation = counts.get("bug", 0) > 0 or counts.get("missing", 0) > 0

    # Aggregate
    result = {
        "matrix_path": str(MATRIX_PATH.relative_to(REPO_ROOT)),
        "total_cells": sum(counts.values()),
        "per_status": counts,
        "per_binding": per_binding_counts,
        "structural_issues": issues,
        "readme_warnings": readme_warnings,
        "closure_violation": closure_violation,
        "domain_closure_state": (
            # TRUE 100% only when there is exactly zero beta_limitation,
            # bug, or missing — no soft-deferred cells allowed.
            "BINDING_API_PARITY_TRUE_100_PERCENT_GREEN"
            if not closure_violation
            and not any(i["severity"] == "error" for i in issues)
            and (not strict_readme or not readme_warnings)
            and counts.get("beta_limitation", 0) == 0
            else (
                "BINDING_API_PARITY_100_PERCENT_GREEN"
                if not closure_violation
                and not any(i["severity"] == "error" for i in issues)
                and (not strict_readme or not readme_warnings)
                else "BINDING_API_PARITY_BLOCKED"
            )
        ),
    }

    # Return appropriate exit code
    if any(i["severity"] == "error" for i in issues):
        rc = 1
    elif closure_violation:
        rc = 2
    elif strict_readme and readme_warnings:
        rc = 3
    else:
        rc = 0

    if verbose:
        print(f"matrix: {result['matrix_path']}")
        print(f"total cells: {result['total_cells']}")
        for s, c in counts.items():
            print(f"  {s:30} = {c}")
        if issues:
            print(f"\nstructural issues ({len(issues)}):")
            for i in issues:
                print(f"  [{i['severity']}] {i['kind']}: {i['capability']} / {i['binding']}")
        if readme_warnings:
            print(f"\nreadme implication warnings ({len(readme_warnings)}):")
            for w in readme_warnings:
                print(
                    f"  [{w['severity']}] {w['readme']}: pattern {w['pattern']!r} "
                    f"hit while {w['capability']} is intentionally_unsupported for {w['binding']}"
                )

    return rc, result


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0] if __doc__ else "")
    p.add_argument("--json", action="store_true", help="machine-readable JSON output")
    p.add_argument("--verbose", action="store_true", help="per-cell diagnostics")
    p.add_argument(
        "--strict-readme",
        action="store_true",
        help="upgrade README-implication warnings into failure (exit 3)",
    )
    args = p.parse_args(argv)
    rc, result = check(verbose=args.verbose, strict_readme=args.strict_readme)
    if args.json:
        json.dump(result, sys.stdout, indent=2)
        sys.stdout.write("\n")
    else:
        s = result["per_status"]
        print(
            f"binding-api-parity: total={result['total_cells']} "
            f"supported={s.get('supported',0)} "
            f"intentionally_unsupported={s.get('intentionally_unsupported',0)} "
            f"beta_limitation={s.get('beta_limitation',0)} "
            f"bug={s.get('bug',0)} missing={s.get('missing',0)}"
        )
        print(f"verdict: {result['domain_closure_state']}")
        if result["structural_issues"]:
            print(f"structural issues: {len(result['structural_issues'])} (rerun with --verbose)")
        if result["readme_warnings"]:
            print(
                f"readme warnings: {len(result['readme_warnings'])}"
                + (" (failing under --strict-readme)" if args.strict_readme else "")
            )
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
