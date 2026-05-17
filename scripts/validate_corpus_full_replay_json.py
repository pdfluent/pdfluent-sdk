#!/usr/bin/env python3
"""
validate_corpus_full_replay_json.py — JSON schema validator for
CORPUS_FULL_REPLAY_RESULT.json produced by the XFA-CORPUS-GATE-FULL-REPLAY
benchmark run.

Exit codes:
  0  valid
  1  schema violations found
"""

import json
import sys
from pathlib import Path


REQUIRED_TOP_LEVEL = {
    "run_date": str,
    "canonical_sha": str,
    "binary_sha256": str,
    "binary_version": str,
    "vps_host": str,
    "subsets": dict,
    "aggregate": dict,
    "verdict": str,
}

REQUIRED_SUBSET = {
    "total_docs": int,
    "success": int,
    "crashes": int,
    "timeouts": int,
    "p50_all_ms": (int, type(None)),
    "p95_all_ms": (int, type(None)),
    "p99_all_ms": (int, type(None)),
    "p50_nonzero_ms": (int, type(None)),
    "p95_nonzero_ms": (int, type(None)),
    "peak_kb_max": int,
    "js_runtime_errors": int,
    "js_resolve_failures": int,
    "js_unsupported_host_calls": int,
    "js_probe_skips": int,
    "js_mutations": int,
    "js_instance_writes": int,
    "js_list_writes": int,
    "js_binding_errors": int,
    "oracle_covered": int,
    "csv_file": str,
}

REQUIRED_AGGREGATE = {
    "total_docs": int,
    "success": int,
    "crashes": int,
    "timeouts": int,
    "crash_rate_pct": (float, int),
    "timeout_rate_pct": (float, int),
    "p50_nonzero_ms": (int, type(None)),
    "p95_nonzero_ms": (int, type(None)),
    "p50_budget_88ms": str,
    "p95_budget_206ms": str,
    "verdict": str,
}

VALID_VERDICTS = {
    "XFA_CORPUS_FULL_REPLAY_GREEN",
    "XFA_CORPUS_FULL_REPLAY_PARTIAL_CRASH",
    "XFA_CORPUS_FULL_REPLAY_PARTIAL_TIMEOUT",
    "XFA_CORPUS_FULL_REPLAY_PARTIAL_PERF_REGRESSION",
    "XFA_CORPUS_FULL_REPLAY_PARTIAL_FORMS_ONLY",
    "XFA_CORPUS_FULL_REPLAY_PARTIAL_NO_GOLDEN",
}

PRIVATE_PATH_PATTERNS = ["/opt/", "/home/", "/root/", "/Users/", "/var/"]


def check_no_private_paths(obj, path=""):
    """Recursively check no private filesystem paths appear in string values."""
    errors = []
    if isinstance(obj, dict):
        for k, v in obj.items():
            errors.extend(check_no_private_paths(v, f"{path}.{k}"))
    elif isinstance(obj, list):
        for i, v in enumerate(obj):
            errors.extend(check_no_private_paths(v, f"{path}[{i}]"))
    elif isinstance(obj, str):
        for pat in PRIVATE_PATH_PATTERNS:
            if pat in obj:
                errors.append(f"  {path}: private path pattern '{pat}' found in value: {obj[:80]!r}")
    return errors


def validate(json_path: Path) -> list[str]:
    errors = []

    try:
        data = json.loads(json_path.read_text())
    except json.JSONDecodeError as e:
        return [f"JSON parse error: {e}"]

    # Top-level fields
    for field, expected_type in REQUIRED_TOP_LEVEL.items():
        if field not in data:
            errors.append(f"Missing top-level field: {field!r}")
        elif not isinstance(data[field], expected_type):
            errors.append(f"Field {field!r}: expected {expected_type.__name__}, got {type(data[field]).__name__}")

    # Subsets
    if "subsets" in data and isinstance(data["subsets"], dict):
        if not data["subsets"]:
            errors.append("'subsets' is empty — at least one subset required")
        for subset_name, subset_data in data["subsets"].items():
            if not isinstance(subset_data, dict):
                errors.append(f"Subset {subset_name!r}: not a dict")
                continue
            for field, expected_type in REQUIRED_SUBSET.items():
                if field not in subset_data:
                    errors.append(f"Subset {subset_name!r}: missing field {field!r}")
                elif not isinstance(expected_type, tuple):
                    if not isinstance(subset_data[field], expected_type):
                        errors.append(f"Subset {subset_name!r}.{field}: expected {expected_type.__name__}, got {type(subset_data[field]).__name__}")
                else:
                    if not isinstance(subset_data[field], expected_type):
                        names = " | ".join(t.__name__ for t in expected_type)
                        errors.append(f"Subset {subset_name!r}.{field}: expected {names}, got {type(subset_data[field]).__name__}")

            # Gate: crash_rate < 25%, timeout_rate < 10%
            total = subset_data.get("total_docs", 0)
            if total > 0:
                cr = subset_data.get("crashes", 0) / total * 100
                tr = subset_data.get("timeouts", 0) / total * 100
                if cr >= 25:
                    errors.append(f"Subset {subset_name!r}: crash_rate {cr:.1f}% >= 25% STOP-RULE VIOLATION")
                if tr >= 10:
                    errors.append(f"Subset {subset_name!r}: timeout_rate {tr:.1f}% >= 10% STOP-RULE VIOLATION")

    # Aggregate
    if "aggregate" in data and isinstance(data["aggregate"], dict):
        for field, expected_type in REQUIRED_AGGREGATE.items():
            if field not in data["aggregate"]:
                errors.append(f"Aggregate: missing field {field!r}")
            elif not isinstance(expected_type, tuple):
                if not isinstance(data["aggregate"][field], expected_type):
                    errors.append(f"Aggregate.{field}: type mismatch")

    # Verdict
    if "verdict" in data:
        v = data["verdict"]
        if not any(v.startswith(vv.split("_PARTIAL")[0]) for vv in VALID_VERDICTS):
            errors.append(f"Unexpected verdict: {v!r}")

    # No private paths
    path_errors = check_no_private_paths(data)
    errors.extend(path_errors)

    return errors


def main():
    if len(sys.argv) < 2:
        print("Usage: validate_corpus_full_replay_json.py <result.json>")
        sys.exit(1)

    path = Path(sys.argv[1])
    if not path.exists():
        print(f"ERROR: file not found: {path}")
        sys.exit(1)

    errors = validate(path)
    if errors:
        print(f"VALIDATION FAILED: {len(errors)} error(s) in {path.name}")
        for e in errors:
            print(f"  {e}")
        sys.exit(1)
    else:
        print(f"VALIDATION PASSED: {path.name}")
        data = json.loads(path.read_text())
        print(f"  Verdict: {data.get('verdict')}")
        print(f"  Subsets: {list(data.get('subsets', {}).keys())}")
        agg = data.get("aggregate", {})
        print(f"  Total:   {agg.get('total_docs')} docs, {agg.get('crashes')} crashes, {agg.get('timeouts')} timeouts")
        sys.exit(0)


if __name__ == "__main__":
    main()
