#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A line of the private term list is text, unless it says otherwise (#222).

`geen_interne_zaken.py` reads customer and partner names from a list outside the
tree and compiled every line as a regular expression. While the list held names
that was invisible: a name carries no metacharacter, so text and pattern are the
same string.

It stopped being invisible on 06-09-2026. #222 put base64 key material on the
list -- 25 lines of a PEM body -- so the seeding of the public repository would
redact it out of the published history instead of publishing a private key. Two
things went wrong at once, and only the first was loud:

  a line beginning with `+`   `re.error: nothing to repeat`, raised inside
                              `alle_regels()`, so not only the partner rule but
                              the message guard, the tree guard and the seeding
                              all died on it
  a `+` further along         no error at all: `v+A` reads as "one or more v",
                              so the term stops matching the text it was added
                              for and the guard reports a clean tree over the
                              one thing it was given to find

The silent half is why this file exists. A crash gets fixed within the hour; a
term that quietly matches nothing is a guard that says OK.

Both kinds of line are asserted here, because either alone is the wrong fix: a
line with a backslash is a pattern its author wrote on purpose and has to keep
working, and every other line has to be matched as the text it is.
"""
from __future__ import annotations

import importlib.util
import os
import pathlib
import re
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = REPO / "scripts" / "ci" / "geen_interne_zaken.py"

# The count the assertions below have to reach. A case that silently stops
# running is indistinguishable from a passing one, which is the failure this
# whole file is about.
MIN_CASES = 12

ran = 0
failed: list[str] = []


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f" -- {detail}" if not ok and detail else ""))
    if not ok:
        failed.append(what)


def load_guard():
    spec = importlib.util.spec_from_file_location("giz_under_test", GUARD)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def rule_for(*terms: str):
    """The partner rule built from a list holding exactly these lines.

    The list is written to a throwaway file and named through the environment
    variable a real run uses, so this exercises the reading and the compiling
    together. Pointing the module's constant at a string would test neither.
    """
    handle = tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False, encoding="utf-8")
    handle.write("# a comment line, which is not a term\n")
    handle.write("\n".join(terms) + "\n")
    handle.close()
    os.environ["PDFLUENT_INTERNE_TERMEN"] = handle.name
    module = load_guard()
    try:
        return module, module.private_regel()
    except re.error as fout:
        # Reported, not propagated: a suite that dies on its first case says
        # nothing about the others, and the silent half of this defect lives in
        # the cases further down.
        print(f"  note  the rule did not compile: {type(fout).__name__}")
        return module, None


def matches(rule, text: str) -> bool:
    return bool(rule[1].search(text))


# 1-3. The line that took the guard down, and the one that was worse.
module, rule = rule_for("+ABCdef/gh")
expect("a term beginning with `+` compiles instead of raising", rule is not None)
if rule is not None:
    expect("and it finds its own text", matches(rule, "key body +ABCdef/gh here"))

module, rule = rule_for("abc+def")
expect("a `+` inside a term is text, so the term is found",
       rule is not None and matches(rule, "see abc+def please"))
expect("and it is NOT read as a repeat, so `abccdef` is not a hit",
       rule is not None and not matches(rule, "see abccdef please"))

# 4-5. The rest of the base64 alphabet, and an edge that is not a word character.
module, rule = rule_for("MIIEv/gIBAD+Q")
expect("a term carrying `/` and `+` is found verbatim",
       rule is not None and matches(rule, "prefix MIIEv/gIBAD+Q suffix"))

module, rule = rule_for("QmFzZTY0bGluZQ==")
expect("a term ending in `=` is found at the end of a line",
       rule is not None and matches(rule, "tail QmFzZTY0bGluZQ=="))

# 6-7. The documented idiom keeps working: a backslash means a pattern.
module, rule = rule_for(r"Instantly\.ai")
expect("a term with a backslash stays a pattern and matches the name",
       rule is not None and matches(rule, "we spoke to [redacted] today"))
expect("and the escaped dot still means a dot, so `instantlyXai` is not a hit",
       rule is not None and not matches(rule, "we spoke to instantlyXai today"))

# 8-10. What the rule did before, and must go on doing, for names.
module, rule = rule_for("Acme")
expect("a name is still bounded, so `Acme` is a hit on its own",
       rule is not None and matches(rule, "the Acme contract"))
expect("and `AcmeCorp` is not one",
       rule is not None and not matches(rule, "the AcmeCorp contract"))
expect("matching is still case-insensitive",
       rule is not None and matches(rule, "the ACME contract"))
expect("the unbounded pattern still finds a name inside a filename",
       rule is not None and bool(module._PRIVE_RX_LOS.search("X_ACME_contract.md")))

# 11-12. No list, no verdict -- the refusal belongs to the caller, not to a
# silent pass, and an empty file is the same case as a missing one.
os.environ["PDFLUENT_INTERNE_TERMEN"] = str(REPO / "does-not-exist-anywhere.txt")
module = load_guard()
expect("a missing list yields no rule", module.private_regel() is None)

empty = tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False, encoding="utf-8")
empty.write("# only a comment\n")
empty.close()
os.environ["PDFLUENT_INTERNE_TERMEN"] = empty.name
module = load_guard()
expect("a list of nothing but comments yields no rule", module.private_regel() is None)

if ran < MIN_CASES:
    print(f"[term-list] FATAL: {ran} case(s) ran, below the floor of {MIN_CASES}. "
          "A suite that stopped running looks exactly like a passing one.",
          file=sys.stderr)
    sys.exit(1)

if failed:
    print(f"\n[term-list] {len(failed)} of {ran} case(s) failed:", file=sys.stderr)
    for what in failed:
        print(f"  - {what}", file=sys.stderr)
    sys.exit(1)

print(f"[term-list] OK: {ran} case(s) -- a line is text unless it carries a "
      "backslash, boundaries hold for names and let a key line match.")
