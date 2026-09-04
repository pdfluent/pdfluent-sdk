#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-PDFluent-Commercial
#
# This file is part of PDFluent. See LICENSE for the AGPL terms and
# LICENSE-COMMERCIAL.md for the commercial alternative.
"""What header_sweep must do, and where it did not.

The last three cases are not an extension but regressions: they record why the
guard looked green enough for months while 615 files carried no header at all.
"""
from __future__ import annotations
import os, pathlib, subprocess, sys, tempfile

CI = pathlib.Path(__file__).resolve().parent
SWEEP = CI / "header_sweep.py"
# The marker of a correct header is the guard's own LICENTIEREGEL, not an SPDX
# expression: `HEADER` does not carry one. A test measuring against a string it
# invented itself measures something other than the guard does.
sys.path.insert(0, str(CI))
from header_sweep import (HEADER as GOOD_HEADER, LICENTIEREGEL as DUAL_LINE,  # noqa: E402
                          MIN_BESTANDEN)
PROP = "This software is proprietary"

PROP_HEADER_RS = ("// Copyright (c) 2026 Innovation Trigger B.V.\n"
               "//\n// This software is proprietary. Unauthorised copying is prohibited.\n")
PROP_HEADER_HASH = ("# Copyright (c) 2026 Innovation Trigger B.V.\n"
                 "#\n# This software is proprietary. Unauthorised copying is prohibited.\n")


def tree_without_padding(tmp: pathlib.Path) -> pathlib.Path:
    """The bare structure, without the files that meet the floor."""
    crate = tmp / "crates" / "own-crate"
    (crate / "src").mkdir(parents=True)
    (crate / "Cargo.toml").write_text('[package]\nname = "own-crate"\n'
                                      'license = "AGPL-3.0-only"\n', encoding="utf-8")
    (tmp / "scripts").mkdir(exist_ok=True)
    (tmp / "benchmarks").mkdir(exist_ok=True)
    return crate


def tree(tmp: pathlib.Path) -> pathlib.Path:
    """A minimal repository: one crate of our own, plus scripts/ for the `#` side."""
    crate = tmp / "crates" / "own-crate"
    (crate / "src").mkdir(parents=True)
    (crate / "Cargo.toml").write_text('[package]\nname = "own-crate"\n'
                                      'license = "AGPL-3.0-only"\n', encoding="utf-8")
    # Get above MIN_BESTANDEN: the guard rightly refuses to judge a tree too
    # small to say anything about, so a fixture that stays under it would test the
    # refusal instead of the behaviour.
    for i in range(MIN_BESTANDEN + 10):
        (crate / "src" / f"pad{i}.rs").write_text(GOOD_HEADER + "pub fn v() {}\n",
                                                  encoding="utf-8")
    (tmp / "scripts").mkdir(exist_ok=True)
    (tmp / "benchmarks").mkdir(exist_ok=True)
    return crate


def run_sweep(tmp: pathlib.Path, *flags: str) -> subprocess.CompletedProcess:
    env = dict(os.environ, PDFLUENT_HEADER_SWEEP_ROOT=str(tmp))
    return subprocess.run([sys.executable, str(SWEEP), *flags],
                          capture_output=True, text=True, env=env)


def case(label: str, holds: bool, why: str) -> bool:
    print(f"  {'ok  ' if holds else 'FOUT'}  {label}")
    if not holds:
        print(f"        {why}")
    return holds


def main() -> int:
    ok_all = True
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        crate = tree(tmp)
        (crate / "src" / "bare.rs").write_text("pub fn a() {}\n", encoding="utf-8")
        (crate / "src" / "prop.rs").write_text(PROP_HEADER_RS + "pub fn b() {}\n", encoding="utf-8")
        (crate / "src" / "attr.rs").write_text("#![allow(dead_code)]\npub fn c() {}\n",
                                               encoding="utf-8")
        (tmp / "scripts" / "prop.py").write_text("#!/usr/bin/env python3\n" + PROP_HEADER_HASH
                                                 + "print(1)\n", encoding="utf-8")
        (tmp / "scripts" / "prop.sh").write_text("#!/bin/sh\n" + PROP_HEADER_HASH + "true\n",
                                                 encoding="utf-8")
        (tmp / "scripts" / "bare.py").write_text("print(2)\n", encoding="utf-8")

        # The term as a string literal, under a correct header. This is the
        # guard's own source: it called itself a file with a proprietary header,
        # because it looked for the words in "the first sixty lines" and its own
        # search term sits there. A header is a header by where it stands.
        (tmp / "scripts" / "zoeker.py").write_text(
            "#!/usr/bin/env python3\n" + GOOD_HEADER.replace("//", "#")
            + '\nTERM = "This software is proprietary"\nprint(TERM)\n', encoding="utf-8")

        r = run_sweep(tmp)
        out = r.stdout + r.stderr
        ok_all &= case("a tree with faults is red", r.returncode == 1, out[:200])

        # THE regression: every category in one run, not just the first.
        #
        # While every category had its own `return 1`, 18 wrong headers meant you
        # never saw that 615 files had no header at all -- the guard jumped out
        # before it got there. A report that stops at the first finding does not
        # tell you how large the problem is.
        #
        # The strings below are Dutch on purpose: they are header_sweep.py's own
        # messages, and that file is one of the 22 Dutch files in scripts/ci.
        ok_all &= case("every category is reported in the same run",
                      "dragen een kop die niet" in out and "geen kop" in out
                      and "`#`-kop" in out,
                      f"not all three reported:\n{out[:400]}")

        # A `.py` without a header is not a finding. The `#` side repairs the
        # contradiction; it does not impose a header on files that never had one.
        ok_all &= case("the term as a string literal is not a header",
                      "zoeker.py" not in out, out[:300])
        ok_all &= case("a `#` file without a header is not a finding",
                      "bare.py" not in out, out[:300])

        r = run_sweep(tmp, "--write")
        ok_all &= case("--write is green", r.returncode == 0, (r.stdout + r.stderr)[:200])

        text_prop = (crate / "src" / "prop.rs").read_text(encoding="utf-8")
        # Replacement, not addition: master's --write left a proprietary header
        # standing because the marker was already there, and then reported "header
        # applied" over files it had not touched.
        ok_all &= case("a proprietary `.rs` header is REPLACED",
                      DUAL_LINE in text_prop and PROP not in text_prop, text_prop[:200])
        ok_all &= case("a bare `.rs` gets the header",
                      DUAL_LINE in (crate / "src" / "bare.rs").read_text(encoding="utf-8"), "")
        text_attr = (crate / "src" / "attr.rs").read_text(encoding="utf-8")
        ok_all &= case("a `#![...]` attribute stays on line 1",
                      text_attr.splitlines()[0] == "#![allow(dead_code)]" and DUAL_LINE in text_attr,
                      text_attr[:200])
        for label in ("prop.py", "prop.sh"):
            t = (tmp / "scripts" / label).read_text(encoding="utf-8")
            ok_all &= case(f"{label}: shebang stays on line 1, header replaced",
                          t.splitlines()[0].startswith("#!") and DUAL_LINE in t and PROP not in t,
                          t[:200])
        ok_all &= case("a `#` file without a header is left alone",
                      (tmp / "scripts" / "bare.py").read_text(encoding="utf-8") == "print(2)\n",
                      "")

        ok_all &= case("after --write the tree is green", run_sweep(tmp).returncode == 0, "")

        # THE regression this cost 400 lines to learn: replace the header, not
        # the comment block it sits in.
        #
        # The first version walked forward while the line started with `#`, so it
        # swallowed the prose underneath -- 13 files lost about 400 lines of
        # explanation between them. The re-run guard saw nothing, because a
        # generator reproduces its own deletion: the second run removes exactly
        # what the first already removed, so the diff is empty. A check that
        # compares a generator against itself cannot see what the generator is
        # wrong about.
        (tmp / "scripts" / "prose.sh").write_text(
            "#!/bin/sh\n" + PROP_HEADER_HASH
            + "#\n# Why this script exists, at length.\n#\n"
              "# A second paragraph that must survive.\n\ntrue\n",
            encoding="utf-8")
        run_sweep(tmp, "--write")
        prose = (tmp / "scripts" / "prose.sh").read_text(encoding="utf-8")
        ok_all &= case("the prose under a replaced header survives",
                       "Why this script exists" in prose
                       and "A second paragraph that must survive" in prose,
                       prose[:300])
        ok_all &= case("and the old header is gone",
                       PROP not in prose and DUAL_LINE in prose, prose[:300])

        # The mutation: put the proprietary header back and the guard must go
        # red. Without this case a test survives the removal of the very thing it
        # guards.
        (crate / "src" / "prop.rs").write_text(PROP_HEADER_RS + "pub fn b() {}\n", encoding="utf-8")
        ok_all &= case("a restored proprietary header turns it red again",
                      run_sweep(tmp).returncode == 1, "")

    # The floor itself: a tree too small to say anything about must be refused,
    # not called green. Without this case someone can remove the floor and
    # everything above stays green -- the test would survive the disappearance of
    # exactly that protection.
    with tempfile.TemporaryDirectory() as d:
        small = pathlib.Path(d)
        crate = tree_without_padding(small)
        (crate / "src" / "one.rs").write_text("pub fn a() {}\n", encoding="utf-8")
        r = run_sweep(small)
        ok_all &= case("a tree too small is refused, not called green",
                      r.returncode != 0 and "ONDERGRENS" in (r.stdout + r.stderr),
                      (r.stdout + r.stderr)[:200])

    print("test_header_sweep: " + ("OK" if ok_all else "GEFAALD"))
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
