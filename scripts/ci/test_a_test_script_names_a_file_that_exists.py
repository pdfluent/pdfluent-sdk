#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The npm-script guard catches a deleted test file, and only that (#515).

The case that decides whether this guard is usable is the third one: the exact
line that shipped read `--testPathIgnorePatterns tests/license` beside two real
paths, and that value is a PATTERN. A guard that demanded it exist would have
been red on a correct manifest and would have been turned off within a day.
"""
from __future__ import annotations

import json
import pathlib
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import a_test_script_names_a_file_that_exists as guard  # noqa: E402

MINIMUM_CASES = 9  # FLOOR


def tree(root: pathlib.Path, scripts: dict, files: tuple[str, ...]) -> pathlib.Path:
    root.mkdir(parents=True, exist_ok=True)
    for name in files:
        target = root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text("//\n", encoding="utf-8")
    (root / "package.json").write_text(
        json.dumps({"name": "@pdfluent/x", "scripts": scripts}, indent=2) + "\n",
        encoding="utf-8")
    return root / "package.json"


def main() -> int:
    failures: list[str] = []
    cases = 0

    def case(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:400]}"))
        if not ok:
            failures.append(what)

    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)

        m = tree(tmp / "ok", {"test": "jest tests/"}, ("tests/smoke.test.js",))
        found, _, _ = guard.judge(tmp, [m])
        case("a script naming a directory that exists passes", found == [], str(found))

        # The line that shipped, minus the deletions.
        m = tree(tmp / "pattern",
                 {"test": "jest tests/ --testPathIgnorePatterns tests/license"},
                 ("tests/smoke.test.js",))
        found, _, _ = guard.judge(tmp, [m])
        case("a --testPathIgnorePatterns value is a pattern, not a file",
             found == [], str(found))

        # The line that shipped.
        m = tree(tmp / "deleted",
                 {"test": "jest tests/ --testPathIgnorePatterns tests/license"
                          " && jest tests/license.test.js"},
                 ("tests/smoke.test.js",))
        found, _, _ = guard.judge(tmp, [m])
        case("a deleted test file named by the script is caught",
             any("tests/license.test.js" in x for x in found), str(found))
        case("and it says why a runner cannot succeed on it",
             any("exits non-zero" in x for x in found), str(found))

        m = tree(tmp / "bare", {"build": "tsc", "lint": "eslint --fix"},
                 ("tests/smoke.test.js",))
        found, _, _ = guard.judge(tmp, [m])
        case("bare commands and flags are not read as paths", found == [], str(found))

        m = tree(tmp / "glob", {"test": "jest tests/*.spec.js"},
                 ("tests/one.spec.js",))
        found, _, _ = guard.judge(tmp, [m])
        case("a glob is judged by the directory before its wildcard",
             found == [], str(found))

        m = tree(tmp / "glob-bad", {"test": "jest suites/*.spec.js"},
                 ("tests/one.spec.js",))
        found, _, _ = guard.judge(tmp, [m])
        case("a glob whose directory is gone is caught",
             any("suites" in x for x in found), str(found))

        m = tree(tmp / "chain",
                 {"test": "node scripts/pre.js && jest tests/ && node scripts/post.js"},
                 ("tests/a.test.js", "scripts/pre.js"))
        found, _, _ = guard.judge(tmp, [m])
        case("every command in a chain is read, not just the first",
             any("scripts/post.js" in x for x in found), str(found))

    print("\n  -- against the real tree --")
    found, manifests_read, checked = guard.judge(guard.REPO, guard.manifests())
    case("this repository's npm scripts all name files that exist",
         found == [] and manifests_read >= guard.MIN_MANIFESTS and checked >= guard.MIN_PATHS,
         f"{found} manifests={manifests_read} checked={checked}")

    if cases < MINIMUM_CASES:
        print(f"\nFAIL: {cases} cases ran, at least {MINIMUM_CASES} were written")
        return 1
    if failures:
        print(f"\nFAIL: {len(failures)} of {cases} cases")
        return 1
    print(f"\nOK: {cases} cases")
    return 0


if __name__ == "__main__":
    sys.exit(main())
