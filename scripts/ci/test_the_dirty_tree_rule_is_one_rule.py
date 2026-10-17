#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""What counts as a dirty tree on the publish path is one rule (#358).

It was two. `prepublish_crate_audit.sh` and `publish_ordered.sh` each carried
their own copy, the audit was taught that its own reports are not dirt, and the
publish script's copy was not. That copy only bites on a RESUME -- the first run
of a publish starts from a clean tree -- so three runs of the 1.0.0 publish went
past it and the fourth stopped on the reports its own previous run had written.

The rule is exercised against a real git repository in a temporary directory,
because what it reads is `git status --porcelain` and a fixture that fakes that
output would be testing the fixture. The last two cases are about the wiring:
one rule is only one rule while both callers use it.
"""
from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from fixture_env import inside_the_sandbox, sealed_env  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[2]
RULE = REPO / "scripts" / "release" / "tree_dirt.sh"
AUDIT = REPO / "scripts" / "release" / "prepublish_crate_audit.sh"
ORDERED = REPO / "scripts" / "release" / "publish_ordered.sh"
REPORTS = "benchmarks/runs/prepublish_audits"

MINIMUM_CASES = 9  # FLOOR


def git(repo: pathlib.Path, *args: str) -> None:
    subprocess.run(["git", *args], cwd=repo, check=True, capture_output=True,
                   env=sealed_env(identity=True, cwd=repo))


def dirt(repo: pathlib.Path, crate: str = "", version: str = "") -> str:
    """What the rule says about this tree, run the way the callers run it."""
    script = f'source "{RULE}"; pdfluent_tree_dirt "{repo}" "{crate}" "{version}"'
    out = subprocess.run(["bash", "-c", script], capture_output=True, text=True,
                         cwd=str(repo), env=sealed_env(identity=True, cwd=repo))
    return out.stdout.strip()


def main() -> int:
    failures: list[str] = []
    cases = 0

    def case(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:300]}"))
        if not ok:
            failures.append(what)

    with tempfile.TemporaryDirectory() as tmp:
        repo = pathlib.Path(tmp) / "tree"
        (repo / REPORTS).mkdir(parents=True)
        # Before the first write. Sealing stops git READING the real checkout;
        # this is what stops it acting on one it finds by walking up.
        inside_the_sandbox(repo.parent)
        git(repo.parent, "init", "-q", "tree")
        git(repo, "config", "user.email", "t@example.invalid")
        git(repo, "config", "user.name", "t")
        # One report already under version control, as 107 of them are.
        old = repo / REPORTS / "pdf-annot-0.9.0.audit.json"
        old.write_text("{}\n", encoding="utf-8")
        (repo / "src.rs").write_text("fn main() {}\n", encoding="utf-8")
        git(repo, "add", "-A")
        git(repo, "commit", "-qm", "base")

        case("a clean tree is clean", dirt(repo) == "", dirt(repo))

        # THE DEFECT, for publish_ordered.sh: the reports a previous run of this
        # very publish wrote. Untracked, in that one directory.
        fresh = repo / REPORTS / "pdf-syntax-0.5.7.audit.json"
        fresh.write_text("{}\n", encoding="utf-8")
        case("an untracked report is not dirt, with no crate named",
             dirt(repo) == "", dirt(repo))
        case("and not with one either",
             dirt(repo, "pdf-syntax", "0.5.7") == "", dirt(repo, "pdf-syntax", "0.5.7"))

        # And it is still a guard about everything else.
        stray = repo / "a_file_somebody_left.txt"
        stray.write_text("x\n", encoding="utf-8")
        case("untracked dirt elsewhere is dirt",
             "a_file_somebody_left.txt" in dirt(repo), dirt(repo))
        stray.unlink()

        # A TRACKED report that changed is a change to the evidence of an
        # earlier release. Without a crate named -- publish_ordered.sh's call,
        # which runs before any audit has rewritten anything -- it is dirt.
        old.write_text('{"edited": true}\n', encoding="utf-8")
        case("a changed tracked report is dirt with no crate named",
             "pdf-annot-0.9.0" in dirt(repo), dirt(repo))
        case("and still dirt when another crate is named",
             "pdf-annot-0.9.0" in dirt(repo, "pdf-syntax", "0.5.7"),
             dirt(repo, "pdf-syntax", "0.5.7"))
        case("and not dirt when it is the one this run rewrites",
             dirt(repo, "pdf-annot", "0.9.0") == "", dirt(repo, "pdf-annot", "0.9.0"))
        old.write_text("{}\n", encoding="utf-8")

    # THE WIRING. One rule is one rule only while both callers use it, and the
    # way this broke was a second copy that did not get the repair.
    audit = AUDIT.read_text(encoding="utf-8")
    ordered = ORDERED.read_text(encoding="utf-8")
    case("both publish scripts source the rule",
         "tree_dirt.sh" in audit and "pdfluent_tree_dirt" in audit
         and "tree_dirt.sh" in ordered and "pdfluent_tree_dirt" in ordered)
    # A second COPY, not a second mention: the audit report carries the rule's
    # name in a sentence, and a check that read that as an implementation would
    # be a check nobody could keep green.
    import re
    running = re.compile(r"\$\([^)]*git[^)]*status --porcelain")
    strays = [name for name, body in (("prepublish_crate_audit.sh", audit),
                                      ("publish_ordered.sh", ordered))
              if running.search(body)]
    case("and neither runs its own git status --porcelain", not strays, str(strays))

    print(f"\n{cases} case(s), {len(failures)} failure(s)")
    if cases < MINIMUM_CASES:
        print(f"FAIL: {cases} cases is under the floor of {MINIMUM_CASES}", file=sys.stderr)
        return 1
    if failures:
        print("FAIL: " + "; ".join(failures), file=sys.stderr)
        return 1
    print("OK: one rule, and both publish scripts use it")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
