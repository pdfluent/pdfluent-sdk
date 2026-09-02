#!/usr/bin/env python3
"""cargo-audit and cargo-deny must accept the same advisories (#294).

Two tools read two files. `.cargo/audit.toml` is cargo-audit's, `deny.toml` is
cargo-deny's, and an accepted advisory has to be written in both -- so one
decision lives in two registers, which is the arrangement that drifts.

It had already drifted. On 02-09-2026 audit.toml accepted five advisories and
deny.toml accepted none, so `cargo-deny check advisories` failed on exactly the
five cargo-audit had been told to accept. That job had been red since 28-08 and
the reason on record was a toolchain error -- which was masking this one, so
nobody had seen the disagreement at all.

The lists are compared here rather than trusted, and the failure says which
file is missing which id. Accepting an advisory in one place and forgetting the
other is then a red check instead of a surprise months later.

WHAT THIS DOES NOT DO

It does not judge whether an acceptance is wise. That is a decision with a
reason written beside it in both files; this only says the two files agree
about which decisions were taken.
"""
from __future__ import annotations
import pathlib, sys, tomllib

WORTEL = pathlib.Path(__file__).resolve().parents[2]
AUDIT = WORTEL / ".cargo" / "audit.toml"
DENY = WORTEL / "deny.toml"


def lees(pad: pathlib.Path) -> set[str] | None:
    if not pad.is_file():
        return None
    try:
        doc = tomllib.loads(pad.read_text())
    except tomllib.TOMLDecodeError as exc:
        print(f"[advisory-lists] FATAL: {pad.name} does not parse: {exc}",
              file=sys.stderr)
        raise SystemExit(2)
    return set(doc.get("advisories", {}).get("ignore", []))


def main() -> int:
    audit, deny = lees(AUDIT), lees(DENY)
    ontbreekt = [p.name for p, v in ((AUDIT, audit), (DENY, deny)) if v is None]
    if ontbreekt:
        # Absence is not agreement: if one file is gone the other's acceptances
        # are unchecked, and reporting that as a pass is the failure this
        # repository keeps finding.
        print(f"[advisory-lists] FATAL: {', '.join(ontbreekt)} missing, so the "
              "two lists cannot be compared. That is not the same as agreeing.",
              file=sys.stderr)
        return 2

    alleen_audit = sorted(audit - deny)
    alleen_deny = sorted(deny - audit)
    if alleen_audit or alleen_deny:
        print("[advisory-lists] FAIL: the two advisory lists disagree.",
              file=sys.stderr)
        for a in alleen_audit:
            print(f"    {a} is accepted in .cargo/audit.toml and not in deny.toml, "
                  "so cargo-deny will fail on it while cargo-audit passes.",
                  file=sys.stderr)
        for d in alleen_deny:
            print(f"    {d} is accepted in deny.toml and not in .cargo/audit.toml, "
                  "so cargo-audit will fail on it while cargo-deny passes.",
                  file=sys.stderr)
        print("\n  One decision, two files, because two tools read two files. "
              "Write the id and its reason in both, or in neither.",
              file=sys.stderr)
        return 1

    print(f"[advisory-lists] OK: {len(audit)} accepted advisory/ies, identical in "
          "both files")
    return 0


if __name__ == "__main__":
    sys.exit(main())
