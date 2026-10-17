#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Two audits in a row, and the first one does not stop the second (#358).

`prepublish_crate_audit.sh` refuses a dirty tree at step 1 and writes three
report files at step 8 into `benchmarks/runs/prepublish_audits/`. So the audit
of crate N left exactly the dirt that made the audit of crate N+1 refuse, and a
publish of 32 crates could never get past the first one it audited. Measured
during the 1.0.0 publish on 16-09-2026:

    [publish] 2/32 pdf-substitute-fonts 1.0.0 -- visible in the index
    [publish] 5/32 pdf-standard-fonts 1.0.0 -- auditing
    error: working tree is dirty. Commit or stash before running the audit.
    ?? benchmarks/runs/prepublish_audits/pdf-substitute-fonts-1.0.0.audit.json

The repair is that the audit stops counting its own output, and nothing else.
Cases 3 and 4 are the half that keeps it a guard: dirt anywhere else still
refuses, and so does a tracked report that changed -- what the repair drops is
untracked output under that one directory plus the three files this run is about
to overwrite (case 5), not the directory.

The crate is audited twice rather than two crates being audited in turn: what
step 1 reads is the directory, not a file name, and a second crate would tie
this test to how far the publish chain has got.
"""
from __future__ import annotations

import gzip
import os
import pathlib
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
AUDIT = REPO / "scripts" / "release" / "prepublish_crate_audit.sh"
REPORTS = REPO / "benchmarks" / "runs" / "prepublish_audits"
# One crate, audited twice, plus a report planted under another name. Two real
# crates would tie this test to how far the publish chain has got:
# `formcalc-interpreter` needs `xfa-dom-resolver =1.0.0` from the index, so it
# cannot be packaged until that one is published, and a test that measures the
# dirty check must not fail on dependency resolution. `xfa-dom-resolver` has no
# workspace dependencies and packages at any point in the chain.
CRATE = "xfa-dom-resolver"

MINIMUM_CASES = 6  # FLOOR


def sealed() -> dict[str, str]:
    """No GIT_* inherited: the pre-push hook exports GIT_DIR, and this test
    runs git against the repository it is standing in."""
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def run_audit(crate: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["bash", str(AUDIT), crate], capture_output=True,
                          text=True, cwd=str(REPO), env=sealed())


def snapshot() -> dict[pathlib.Path, bytes]:
    """Every report file and its contents, right now.

    Contents, not names: this directory already holds 107 tracked reports from
    earlier releases, and once this release's own reports are committed --
    which the protocol requires -- `run_audit` rewrites tracked paths that are
    already in the snapshot. Deleting what is new is then not enough; what was
    there has to go back byte for byte, or the test leaves a modified checkout
    behind and the next landing gate refuses it. (codex, #1916)
    """
    if not REPORTS.is_dir():
        return {}
    return {q: q.read_bytes() for q in REPORTS.glob("*") if q.is_file()}


def restore(before: dict[pathlib.Path, bytes]) -> None:
    """Put the directory back exactly as `before` found it."""
    for path in REPORTS.glob("*") if REPORTS.is_dir() else []:
        if path.is_file() and path not in before:
            path.unlink(missing_ok=True)
    for path, body in before.items():
        if not path.exists() or path.read_bytes() != body:
            path.write_bytes(body)


def main() -> int:
    failures: list[str] = []
    cases = 0

    def case(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:500]}"))
        if not ok:
            failures.append(what)

    dirty = subprocess.run(["git", "status", "--porcelain"], capture_output=True,
                           text=True, cwd=str(REPO), env=sealed()).stdout.strip()
    if dirty:
        print("SKIPPED (not a pass): this test needs a clean tree to measure a "
              f"dirty one; found:\n{dirty[:500]}", file=sys.stderr)
        return 1

    before = snapshot()
    stray = REPO / "a_file_this_test_made.txt"
    # A tracked report, to show that the narrowing is to untracked output and
    # not to the directory. Its contents are in `before` and go back there.
    tracked = sorted(
        q for q in before
        if subprocess.run(["git", "ls-files", "--error-unmatch", str(q)],
                          capture_output=True, cwd=str(REPO),
                          env=sealed()).returncode == 0)
    try:
        first = run_audit(CRATE)
        case(f"the first audit ({CRATE}) passes",
             first.returncode == 0, first.stdout[-500:] + first.stderr[-500:])

        # A report under a name this audit will not touch, so what is excluded
        # is shown to be the directory and not the file the audit rewrites.
        planted = REPORTS / "a-crate-audited-earlier-1.0.0.audit.json"
        planted.write_text("{}\n", encoding="utf-8")

        # THE DEFECT: on the old script this is where it stopped.
        second = run_audit(CRATE)
        case("the second audit passes with the first one's reports still there",
             second.returncode == 0
             and "working tree is dirty" not in (second.stdout + second.stderr),
             second.stdout[-500:] + second.stderr[-500:])

        # And it is still a guard about everything else.
        stray.write_text("not an audit report\n", encoding="utf-8")
        third = run_audit(CRATE)
        case("dirt that is not an audit report still refuses",
             third.returncode != 0
             and "working tree is dirty" in (third.stdout + third.stderr),
             third.stdout[-500:] + third.stderr[-500:])
        stray.unlink(missing_ok=True)

        # The half a pathspec exclusion would have thrown away with the rest:
        # a report already under version control, changed. That is a change to
        # the evidence of an earlier release, and it still refuses.
        if not tracked:
            case("a changed tracked report still refuses", False,
                 f"no tracked report in {REPORTS}")
        else:
            victim = tracked[0]
            victim.write_bytes(before[victim] + b"\nedited by a test\n")
            fourth = run_audit(CRATE)
            victim.write_bytes(before[victim])
            case("a changed tracked report still refuses",
                 fourth.returncode != 0
                 and "working tree is dirty" in (fourth.stdout + fourth.stderr),
                 fourth.stdout[-500:] + fourth.stderr[-500:])

        # And the other side of that: THIS crate's own three reports, tracked
        # and rewritten. Once a release's reports are committed, re-auditing
        # that version rewrites them (the report carries a fresh temporary
        # path), which reads as ` M` and would stop the next crate in the
        # train. Staging them is what makes them tracked without a commit.
        mine = sorted(REPORTS.glob(f"{CRATE}-*"))
        staged = subprocess.run(["git", "add", "--"] + [str(q) for q in mine],
                                capture_output=True, text=True, cwd=str(REPO),
                                env=sealed())
        try:
            fifth = run_audit(CRATE)
            case("this crate's own reports do not refuse when tracked",
                 staged.returncode == 0 and fifth.returncode == 0
                 and "working tree is dirty" not in (fifth.stdout + fifth.stderr),
                 (staged.stderr + fifth.stdout[-500:] + fifth.stderr[-500:]))
        finally:
            subprocess.run(["git", "reset", "-q", "--"] + [str(q) for q in mine],
                           capture_output=True, cwd=str(REPO), env=sealed())

        # A STALE, LARGER .crate WHERE CARGO WILL WRITE (#591). cargo does not
        # truncate the file it packages into, so a pool instance whose target/
        # kept an earlier run's package got the old tail behind the new gzip
        # stream, and GNU tar refused it. Planted bigger than any real one, in
        # bytes no gzip reader accepts as a second member. The directory is
        # where cargo puts it -- CARGO_TARGET_DIR when set, not the default.
        pkgdir = REPO / "target" / "package"
        if os.environ.get("CARGO_TARGET_DIR"):
            pkgdir = pathlib.Path(os.environ["CARGO_TARGET_DIR"]) / "package"
        produced = sorted(pkgdir.glob(f"{CRATE}-*.crate"))
        if not produced:
            case("a stale larger .crate does not poison the next audit", False,
                 "the audits above produced no .crate to plant over")
        else:
            crate_file = produced[-1]
            crate_file.write_bytes(os.urandom(4 * 1024 * 1024))
            sixth = run_audit(CRATE)
            try:
                gzip.decompress(crate_file.read_bytes())
                clean, why = True, ""
            except Exception as exc:  # noqa: BLE001 -- any refusal is the answer
                clean, why = False, repr(exc)
            case("a stale larger .crate does not poison the next audit",
                 sixth.returncode == 0 and clean,
                 why + sixth.stdout[-400:] + sixth.stderr[-400:])
    finally:
        stray.unlink(missing_ok=True)
        restore(before)

    print(f"\n{cases} case(s), {len(failures)} failure(s)")
    if cases < MINIMUM_CASES:
        print(f"FAIL: {cases} cases is under the floor of {MINIMUM_CASES}", file=sys.stderr)
        return 1
    if failures:
        print("FAIL: " + "; ".join(failures), file=sys.stderr)
        return 1
    print("OK: an audit does not dirty the tree for the next one")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
