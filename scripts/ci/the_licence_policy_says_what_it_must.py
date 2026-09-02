#!/usr/bin/env python3
"""The licence policy's own lists, guarded (#300).

The gates check that `docs/LICENSE_POLICY.toml` and `deny.toml` AGREE WITH EACH
OTHER. Nothing checked what the policy actually says. Measured on master
01-09-2026: remove both AGPL-3.0 spellings from `licenses.forbidden` and
`test_license_gate.py`, `license_gate.py` and `license_boundary.py` all stay
green. The list could be emptied and every licence gate would report success.

That mattered immediately, because the `AGPL-3.0-only` flip (#257) edits exactly
those strings, and the proof offered for it was "the gate is green afterwards" --
which it would have been whether the forbidden list survived the flip or was
emptied by it.

Same family as #295: the guards check the boundary BETWEEN files rather than the
contents that make the boundary mean anything.

WHAT THIS ASSERTS

  1. Entries the policy cannot do without are present, each with the reason IN
     the assertion rather than in a comment beside it. A comment does not fail.
  2. Every licence named `forbidden` is actually refused by the evaluator, and
     every licence named `allowed` is actually accepted by it. The list and the
     code that reads it cannot drift apart, which is the failure a list-only
     check would still permit: an entry that is present and ignored.

Both directions are covered by test_the_licence_policy_says_what_it_must.py:
removing a required entry fails, and so does an evaluator that stops honouring
one.
"""
from __future__ import annotations
import pathlib, re, sys, tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
POLICY = REPO / "docs" / "LICENSE_POLICY.toml"

# Required, with the reason as data. Each is here because losing it would let
# something specific through, and that sentence is what a reader needs when the
# guard fails.
MUST_FORBID: dict[str, str] = {
    "AGPL-3.0-only": (
        "our own packages declare this after the #257 flip, so it is the "
        "spelling most likely to be edited by accident -- and an AGPL "
        "DEPENDENCY still reaches the whole product, which is the opposite case"
    ),
    "AGPL-3.0-or-later": (
        "the spelling our packages used before #257. It must stay forbidden as "
        "a DEPENDENCY licence -- our own packages declaring it is the opposite "
        "case and is judged elsewhere -- and it is the one a half-finished flip "
        "would remove first"
    ),
    "GPL-2.0-only": "linking it into a product we license commercially is the case this list exists for",
    "GPL-3.0-only": "as GPL-2.0-only, and it adds the anti-tivoisation terms",
    "SSPL-1.0": "not OSI-approved and its service clause reaches anything we host",
}

MUST_ALLOW: dict[str, str] = {
    "MIT": "the most common licence in our dependency graph; losing it fails everything at once",
    "Apache-2.0": "as MIT, and it carries the patent grant we rely on",
}

# A CLASS, not a list of remembered spellings.
#
# The list-of-names form above WAS the defect. `MUST_FORBID` named the four
# `-only` spellings and the two AGPL ones; `GPL-2.0-or-later` and
# `GPL-3.0-or-later` were in the policy but not in the guard. Measured
# 02-09-2026 on this branch: move both to `licenses.allowed` and add them to
# deny.toml's allow list -- the complete, self-consistent edit -- and
# `the_licence_policy_says_what_it_must`, `license_gate`, `license_boundary`
# and both test suites are ALL green with strong copyleft an accepted
# dependency licence. The floor is still met, the sets do not overlap, and the
# evaluator agrees with the policy, because the policy is what changed.
#
# Same shape as the four spellings of the head ref (#1636): a list of names
# where a class belongs. So the family is generated, and a spelling nobody
# typed here is an error rather than a hole. (codex, #1656)
# Not anchored at the start, because a licence field is an EXPRESSION and a
# name can be embedded in one: `MIT OR GPL-3.0-only` is a disjunction whose
# second operand is the GPL, and `LicenseRef-AGPL-3.0` is the family wearing a
# reference's clothes. `^` saw neither. The lookbehind is what keeps LGPL out:
# `LGPL-2.1-only` contains `GPL-2`, and treating file-level copyleft as strong
# would contradict the deliberate decision to classify it per surface.
STERK_COPYLEFT = re.compile(r"(?<![A-Za-z])A?GPL-\d", re.IGNORECASE)
BESTANDS_COPYLEFT = re.compile(r"(?<![A-Za-z])LGPL-\d", re.IGNORECASE)

_STAMMEN = ("GPL-1.0", "GPL-2.0", "GPL-3.0", "AGPL-1.0", "AGPL-3.0")
# Including the bare and `+` spellings. They are SPDX-deprecated and that is
# exactly why they matter: deprecated ids stay in the wild, `AGPL-3.0` is the
# dominant spelling in npm and PyPI metadata, and a deprecated id nobody
# classified reports as unknown -- which stops refusing the moment somebody
# adds it to a list that permits.
_STAARTEN = ("-only", "-or-later", "", "+")
MOET_GECLASSIFICEERD = frozenset(f"{k}{s}" for k in _STAMMEN for s in _STAARTEN)

# An exception can genuinely change the answer -- `GPL-2.0-only WITH
# Classpath-exception-2.0` does not reach a linking user the way the bare GPL
# does -- but each one is a decision somebody made, so they are NAMED. Matching
# `WITH .*` as a pattern would admit any unexamined exception on the strength
# of the word appearing, which is the same mistake one level down.
UITZONDERINGEN: dict[str, str] = {}

# The hand-curated half, ENUMERATED. Not counted.
#
# A floor of six over seven entries let any one of them move to `allowed` --
# with the deny.toml update, the whole edit -- past this guard, the evaluator
# and every other licence check. Which is the same mistake as the one it
# replaced, one round later and by my hand: `MINIMUM_FORBIDDEN` was dead
# because a counter guards an amount and never the contents. Swapping a dead
# counter for a live one does not change what a counter is.
#
# These have no generator, because they share no pattern: they are seven
# separate decisions. So they are seven separate names, each with the reason in
# the assertion, and adding an eighth is a deliberate edit to code that goes
# through review -- which is what the register is for.
MOET_OVERIG_VERBODEN: dict[str, str] = {
    "SSPL-1.0": "not OSI-approved, and its service clause reaches anything we host",
    "BUSL-1.1": "source-available with a use limitation; it is not open source and "
                "the change date is the vendor's to move",
    "Elastic-2.0": "as BUSL-1.1: it forbids offering the software as a service, "
                   "which is what a PDF API is",
    "EUPL-1.2": "copyleft with a compatibility list that pulls in the AGPL, so "
                "allowing it allows that by another route",
    "CDDL-1.0": "file-level copyleft with a patent-retaliation clause, and its "
                "combination with the GPL is unsettled -- we take neither side",
    "CDDL-1.1": "as CDDL-1.0; both spellings are in the wild",
    "CC-BY-SA-4.0": "a content licence whose share-alike reaches derived works; "
                    "it belongs on documents, never in a dependency graph",
}


def main() -> int:
    if not POLICY.is_file():
        print(f"[policy] FATAL: {POLICY} is missing. Refusing to report a clean "
              "policy for a file that is not there.", file=sys.stderr)
        return 2
    try:
        pol = tomllib.loads(POLICY.read_text())
    except tomllib.TOMLDecodeError as exc:
        print(f"[policy] FATAL: {POLICY} does not parse: {exc}", file=sys.stderr)
        return 2

    lic = pol.get("licenses") or {}
    forbidden = set(lic.get("forbidden") or [])
    allowed = set(lic.get("allowed") or [])
    weak = set(lic.get("weak_copyleft") or [])

    problems: list[str] = []

    for name, why in MOET_OVERIG_VERBODEN.items():
        if name not in forbidden:
            problems.append(f"{name} is not in licenses.forbidden. It must be: {why}.")
        if name in allowed:
            problems.append(f"{name} is in licenses.allowed. It must be forbidden: {why}.")

    for name, why in MUST_FORBID.items():
        if name not in forbidden:
            problems.append(f"{name} is not in licenses.forbidden. It must be: {why}.")
    # The family, generated. Absence is the finding: a spelling that is in no
    # list at all reports "unknown" rather than "forbidden", and unknown stops
    # refusing the moment somebody adds it to `allowed`.
    for name in sorted(MOET_GECLASSIFICEERD - forbidden):
        problems.append(
            f"{name} is not in licenses.forbidden. Every SPDX spelling of the "
            "strong-copyleft family must be classified there; this one is not, "
            "so a dependency declaring it reports as unclassified.")

    # And the move that the name list could not see: the family turning up on
    # the side that permits.
    # EVERY list that can permit, not just `allowed`.
    #
    # `weak_copyleft` is the second door and it reads as harmless: put
    # `AGPL-3.0` there, name it in one surface's weak set, and add it to
    # deny.toml -- the complete edit -- and all five licence guards are green
    # with toegestaan() returning True. Worse, license_gate builds the cargo
    # weak set as the UNION of every surface, so naming it on `editor` alone
    # makes it acceptable for all 786 cargo packages. A family checked on one
    # list is a family that moves to another. (codex, #1656)
    per_lijst = [("licenses.allowed", allowed), ("licenses.weak_copyleft", weak)]
    for surface, blok in (pol.get("surfaces") or {}).items():
        if isinstance(blok, dict):
            per_lijst.append((f"surfaces.{surface}.weak_copyleft",
                              set(blok.get("weak_copyleft") or [])))

    for waar, namen in per_lijst:
        for name in sorted(namen):
            if STERK_COPYLEFT.search(name) and name not in UITZONDERINGEN:
                problems.append(
                    f"{name} is in {waar}. Strong copyleft is not a permitted "
                    "DEPENDENCY licence at any spelling and on no list -- our own "
                    "packages declaring AGPL-3.0-only is the opposite case and is "
                    "judged elsewhere. If an exception makes this one different, "
                    "name it in UITZONDERINGEN with the reason; the word WITH is "
                    "not by itself a reason.")

    for name in sorted(allowed):
        if BESTANDS_COPYLEFT.search(name):
            problems.append(
                f"{name} is in licenses.allowed. File-level copyleft is decided "
                "per surface, so it belongs in weak_copyleft (or forbidden) -- "
                "`allowed` is tested first and skips the surface decision.")

    for name, why in MUST_ALLOW.items():
        if name not in allowed:
            problems.append(f"{name} is not in licenses.allowed. It must be: {why}.")

    for name in sorted(forbidden & allowed):
        problems.append(f"{name} is in BOTH allowed and forbidden. The evaluator "
                        "takes forbidden first, so the allow entry is a lie that "
                        "reads as permission.")

    # The list that PERMITS is as much an attack surface as the list that
    # forbids, and this one is worse because it reads as harmless.
    #
    # license_gate.toegestaan() tests `allowed` BEFORE `weak_copyleft`, so a
    # weak-copyleft licence sitting in `allowed` is acceptable everywhere --
    # including on a surface whose weak set is empty, which is the whole
    # mechanism by which MPL-2.0 is permitted in one place and not another.
    # Measured: with MPL-2.0 added to `allowed`, toegestaan() returns True for a
    # surface with weak_allowed=set(), and False without it. The per-surface
    # control is simply skipped. (codex, #1656)
    for name in sorted(allowed & weak):
        problems.append(
            f"{name} is in BOTH allowed and weak_copyleft. `allowed` is tested "
            "first, so this makes it acceptable on every surface -- including "
            "the ones whose weak-copyleft set is deliberately empty. A licence "
            "that needs a per-surface decision cannot also be unconditionally "
            "allowed.")
    for name in sorted(forbidden & weak):
        problems.append(
            f"{name} is in BOTH forbidden and weak_copyleft. One of the two is "
            "wrong: a licence cannot be refused everywhere and permitted per "
            "surface, and which the evaluator honours depends on the order it "
            "happens to test them in.")

    # The list and the code that reads it, checked against each other. An entry
    # that is present and ignored passes every list-only check ever written.
    sys.path.insert(0, str(REPO / "scripts" / "ci"))
    try:
        import license_gate  # noqa: E402
    except Exception as exc:  # pragma: no cover - import failure is the finding
        print(f"[policy] FATAL: cannot import license_gate to check the lists "
              f"against the evaluator: {exc}", file=sys.stderr)
        return 2

    checked = 0
    for name in sorted(forbidden):
        ok, reason = license_gate.toegestaan(name, allowed, weak, forbidden, set())
        checked += 1
        if ok:
            problems.append(f"{name} is listed forbidden and the evaluator accepts "
                            "it anyway. A list the code does not honour is a "
                            "comment.")
        elif "forbidden" not in reason:
            # `False` is the absence of a verdict, not a verdict. Delete the
            # forbidden branch from toegestaan() and every forbidden licence
            # falls through to "not classified" -- also False, so a check that
            # only asked "is it refused" reported agreement while the evaluator
            # had stopped consulting the list at all. That is precisely the
            # regression this guard exists for, and it was the one thing it
            # could not see. (codex, #1656)
            problems.append(
                f"{name} is refused for the wrong reason: {reason!r}. It is on "
                "the forbidden list, so the evaluator should say so -- being "
                "unclassified refuses it today and stops refusing it the moment "
                "somebody adds it to `allowed`.")
    for name in sorted(allowed):
        ok, reason = license_gate.toegestaan(name, allowed, weak, forbidden, set())
        checked += 1
        if not ok:
            problems.append(f"{name} is listed allowed and the evaluator refuses "
                            f"it: {reason}.")

    if checked == 0:
        print("[policy] FATAL: judged no licences at all. A scan that reads "
              "nothing cannot report a clean policy.", file=sys.stderr)
        return 2

    if problems:
        print(f"[policy] FAIL: {len(problems)} problem(s) in {POLICY.name}:",
              file=sys.stderr)
        for p in problems:
            print(f"    {p}", file=sys.stderr)
        print("\n  These lists are the licence policy. Nothing else states it, and "
              "until\n  this guard existed they could be emptied with every gate "
              "still green.", file=sys.stderr)
        return 1

    print(f"[policy] OK: {len(forbidden)} forbidden and {len(allowed)} allowed "
          f"entries; {len(MUST_FORBID) + len(MUST_ALLOW)} required ones present, "
          f"and the evaluator agrees with all {checked}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
