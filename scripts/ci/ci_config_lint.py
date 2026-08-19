#!/usr/bin/env python3
"""Check .gitlab-ci.yml for faults that valid YAML happily hides.

WHY THIS EXISTS

`yaml.safe_load` returning without an exception says the file is YAML. It says
nothing about whether GitLab can run it. On 2026-08-19 this line

    - echo "[binding-python] own target dir: ${CARGO_TARGET_DIR}"

parsed cleanly and produced a *dictionary* inside the script list, because a plain
YAML scalar containing ": " is a mapping. GitLab rejected the whole file, the
pipeline came back with status `failed` and ZERO jobs, and nothing in the job list
pointed at the cause. My local check had said "yaml ok".

So this checks the shape, not just the syntax. Every gate we have is worthless if
the pipeline that runs them will not start.

Exit codes:
    0  fine
    1  a fault that would break the pipeline
    2  could not run
"""

from __future__ import annotations

import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
CI = REPO / ".gitlab-ci.yml"


def flatten(item) -> list:
    """GitLab allows nested arrays of strings; flatten to check the leaves."""
    if isinstance(item, list):
        out = []
        for sub in item:
            out.extend(flatten(sub))
        return out
    return [item]


def main() -> None:
    try:
        import yaml
    except ImportError:
        print("[ci_config_lint] FATAL: pyyaml missing", file=sys.stderr)
        sys.exit(2)

    try:
        doc = yaml.safe_load(CI.read_text())
    except Exception as e:  # noqa: BLE001
        print(f"[ci_config_lint] FAIL: not valid YAML at all: {e}")
        sys.exit(1)

    problems: list[str] = []
    job_names = {n for n, j in doc.items() if not n.startswith(".") and isinstance(j, dict)}

    for name, job in doc.items():
        if not isinstance(job, dict):
            continue
        for key in ("script", "before_script", "after_script"):
            block = job.get(key)
            if block is None:
                continue
            if isinstance(block, str):
                continue
            if not isinstance(block, list):
                problems.append(f"{name}.{key} is a {type(block).__name__}, expected a list")
                continue
            for leaf in flatten(block):
                if isinstance(leaf, str):
                    continue
                # This is the ": " trap. Name it precisely, because the symptom
                # (a pipeline with no jobs) points nowhere near the cause.
                hint = ""
                if isinstance(leaf, dict):
                    k = next(iter(leaf), "")
                    hint = (f'  -- looks like an unquoted ": " in: {k!r}. '
                            "Wrap the whole command in single quotes.")
                problems.append(
                    f"{name}.{key} contains a {type(leaf).__name__}, not a string.{hint}"
                )

        # A `needs` on a job that does not exist stops the pipeline from being
        # created at all, with the same unhelpful "failed, no jobs" symptom.
        for dep in job.get("needs") or []:
            dep_name = dep.get("job") if isinstance(dep, dict) else dep
            if isinstance(dep_name, str) and dep_name not in job_names:
                problems.append(f"{name}.needs refers to {dep_name!r}, which is not a job")

    if problems:
        print(f"[ci_config_lint] FAIL: {len(problems)} problem(s) GitLab would reject:")
        for p in problems:
            print(f"  - {p}")
        print()
        print("[ci_config_lint] These parse as valid YAML. GitLab still refuses them,")
        print("[ci_config_lint] and the failure arrives as a pipeline with no jobs at all.")
        sys.exit(1)

    print(f"[ci_config_lint] {len(job_names)} jobs, all script entries are strings, "
          "all needs resolve")
    sys.exit(0)


if __name__ == "__main__":
    main()
