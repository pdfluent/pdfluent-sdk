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
import pathlib, sys, tomllib

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

MINIMUM_FORBIDDEN = 10  # FLOOR


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

    if len(forbidden) < MINIMUM_FORBIDDEN:  # FLOOR
        problems.append(
            f"licenses.forbidden holds {len(forbidden)} entries; the floor is "
            f"{MINIMUM_FORBIDDEN}. A list this short has been emptied, not curated."
        )

    for name, why in MUST_FORBID.items():
        if name not in forbidden:
            problems.append(f"{name} is not in licenses.forbidden. It must be: {why}.")
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
        ok, _ = license_gate.toegestaan(name, allowed, weak, forbidden, set())
        checked += 1
        if ok:
            problems.append(f"{name} is listed forbidden and the evaluator accepts "
                            "it anyway. A list the code does not honour is a "
                            "comment.")
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
