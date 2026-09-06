#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The contribution guard refuses what it should, and only that (#233).

Two halves. The fixtures below plant each failure the guard exists for -- a
missing file, an emptied template, a promise of money in a security policy, a
pointer at a file the seeding strips -- and prove it goes red on each. The last
block runs the guard against the real tree, so deleting `SECURITY.md`, emptying
`CODEOWNERS` or dropping the sign-off line out of an issue form turns this red
rather than leaving a register that describes files nobody has.

No network, and no seeding: the only thing the guard needs from outside is the
list of stripped paths, which every case passes in.
"""
from __future__ import annotations

import pathlib
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import public_contribution_files_are_complete as guard  # noqa: E402

failures: list[str] = []
ran = 0

CONTRIBUTING = "Fork, branch, sign off with `git commit -s`, which certifies " \
               "the Developer Certificate of Origin.\n"
SECURITY = """# Security Policy

Report to security@pdfluent.com. Do not open a public GitHub issue for a
vulnerability.

### There is no bug bounty

No payment and no reward is offered for a report.
"""
PR_TEMPLATE = "- [ ] Every commit is signed off (`git commit -s`)\n"
CODEOWNERS = "# one maintainer\n* @jasperdew\n"
CONFIG = """blank_issues_enabled: false
contact_links:
  - name: Security vulnerability
    url: https://example.invalid/SECURITY.md
"""
BUG_FORM = "name: Bug report\n# see SECURITY.md\n# sign off with `git commit -s`\n"
FEATURE_FORM = "name: Feature request\n# sign off with `git commit -s`\n"


def case(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:300]}"))
    if not ok:
        failures.append(what)


def tree(root: pathlib.Path, **overrides: str | None) -> pathlib.Path:
    """A complete contribution tree, with named files replaced or removed."""
    files = {
        "CONTRIBUTING.md": CONTRIBUTING,
        "SECURITY.md": SECURITY,
        ".github/PULL_REQUEST_TEMPLATE.md": PR_TEMPLATE,
        ".github/CODEOWNERS": CODEOWNERS,
        ".github/ISSUE_TEMPLATE/config.yml": CONFIG,
        ".github/ISSUE_TEMPLATE/bug_report.yml": BUG_FORM,
        ".github/ISSUE_TEMPLATE/feature_request.yml": FEATURE_FORM,
    }
    files.update(overrides)
    for name, text in files.items():
        if text is None:
            continue
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
    return root


def main() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        base = pathlib.Path(tmp)

        # --- the shape the guard is supposed to accept -----------------------
        fatal, reported = guard.judge(tree(base / "whole"), [])
        case("a complete tree passes", not fatal, str(fatal))
        case("and says what it looked at", bool(reported), str(reported))

        # --- one file at a time ----------------------------------------------
        fatal, _ = guard.judge(tree(base / "no-security", **{"SECURITY.md": None}), [])
        case("a missing SECURITY.md is fatal",
             any("SECURITY.md is missing" in f for f in fatal), str(fatal))

        fatal, _ = guard.judge(
            tree(base / "no-signoff",
                 **{"CONTRIBUTING.md": "Fork, branch, open a pull request.\n"}), [])
        case("a CONTRIBUTING that stops naming the sign-off is fatal",
             any("git commit -s" in f for f in fatal), str(fatal))

        fatal, _ = guard.judge(
            tree(base / "empty-pr", **{".github/PULL_REQUEST_TEMPLATE.md": "## What\n"}), [])
        case("an emptied pull-request template is fatal",
             any("PULL_REQUEST_TEMPLATE" in f for f in fatal), str(fatal))

        fatal, _ = guard.judge(
            tree(base / "comment-owners", **{".github/CODEOWNERS": "# nobody yet\n"}), [])
        case("a CODEOWNERS with only comments is fatal",
             any("no rule" in f for f in fatal), str(fatal))

        fatal, _ = guard.judge(
            tree(base / "blank-issues",
                 **{".github/ISSUE_TEMPLATE/config.yml":
                    "blank_issues_enabled: true\n# SECURITY.md\n"}), [])
        case("re-enabling blank issues is fatal",
             any("blank_issues_enabled" in f for f in fatal), str(fatal))

        fatal, _ = guard.judge(
            tree(base / "unsigned-form",
                 **{".github/ISSUE_TEMPLATE/feature_request.yml": "name: Feature\n"}), [])
        case("an issue form that drops the sign-off is fatal",
             any("feature_request" in f for f in fatal), str(fatal))

        fatal, _ = guard.judge(
            tree(base / "one-form",
                 **{".github/ISSUE_TEMPLATE/feature_request.yml": None}), [])
        case("a single issue form is below the floor",
             any("issue form" in f for f in fatal), str(fatal))

        # --- the promise a security policy must not make ----------------------
        paying = SECURITY + "\nWe pay a bounty of 500 euro for a critical report.\n"
        fatal, _ = guard.judge(tree(base / "paying", **{"SECURITY.md": paying}), [])
        case("a bounty promised in passing is fatal",
             any("promises a payment" in f for f in fatal), str(fatal))

        quiet = SECURITY.replace("### There is no bug bounty\n\n"
                                 "No payment and no reward is offered for a report.\n", "")
        fatal, _ = guard.judge(tree(base / "quiet", **{"SECURITY.md": quiet}), [])
        case("saying nothing about a bounty is fatal too",
             any("no bug bounty" in f for f in fatal), str(fatal))

        # --- what the seeding does to it -------------------------------------
        # `.gitlab-ci.yml` is the real regression: SECURITY.md described the fuzz
        # schedule by naming it, and the seed strips it, so the published copy
        # sent the reader to a file that is not there.
        pointing = SECURITY + "\nThe fuzz jobs are defined in `.gitlab-ci.yml`.\n"
        fatal, _ = guard.judge(tree(base / "pointing", **{"SECURITY.md": pointing}),
                               [".gitlab-ci.yml"])
        case("a pointer at a path the seeding strips is fatal",
             any("points at .gitlab-ci.yml" in f for f in fatal), str(fatal))

        fatal, _ = guard.judge(tree(base / "stripped"), ["CONTRIBUTING.md"])
        case("a contribution file the seeding strips is fatal",
             any("stripped by the seeding" in f for f in fatal), str(fatal))

        fatal, _ = guard.judge(tree(base / "stripped-dir"), [".github/"])
        case("stripping the whole .github directory is fatal",
             sum("stripped by the seeding" in f for f in fatal) >= 4, str(fatal))

        # --- a register that has emptied itself ------------------------------
        held = guard.REQUIRED
        try:
            guard.REQUIRED = {"CONTRIBUTING.md": []}
            fatal, _ = guard.judge(tree(base / "floor"), [])
            case("a register below the floor is fatal",
                 any("floor" in f for f in fatal), str(fatal))
        finally:
            guard.REQUIRED = held

    # --- and the tree this actually ships ------------------------------------
    fatal, _ = guard.judge(guard.REPO, guard.internal_paths())
    case("the real tree satisfies the guard", not fatal, str(fatal))

    print(f"\n[contribution-test] {ran - len(failures)}/{ran} passed")
    if failures:
        for f in failures:
            print(f"  FAILED: {f}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
