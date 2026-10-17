#!/usr/bin/env python3
"""An advisory fixed by a version bump must not come back (#460).

There are two ways to close a RustSec advisory: accept it, with a reason, in
`.cargo/audit.toml` and `deny.toml` -- which `the_advisory_lists_agree.py`
already checks -- or bump past it. The second way leaves no record at all. The
only trace is a version in `Cargo.lock`, and a lockfile is rewritten by any
`cargo update`, a dependency loosening its own requirement, or a merge picking
the other side.

That is not hypothetical here: the reasons for the four `rustls-webpki`
acceptances were removed on 08-09-2026 because the tree had moved past them,
and RUSTSEC-2026-0185 (`quinn-proto`, remote memory exhaustion) was found on
08-09 only because `cargo audit` reads the whole lockfile while `cargo deny`
walks the feature graph and never saw the crate. Neither tool runs offline, and
neither runs on a pull request.

This does. It reads `Cargo.lock` and refuses a version below the floor an
advisory was fixed at, by name, with the reason beside it -- so a regression is
a red check on the change that caused it rather than a finding on a wave two
hours later.

It also refuses an id that is both listed here and accepted in the ignore
lists: fixed and suppressed are different decisions, and a crate cannot be
under both.

Exit codes:
  0 -- every floor holds
  1 -- a crate fell below a floor, or an id is in two registers at once
  2 -- Cargo.lock is missing or unreadable
"""
from __future__ import annotations

import pathlib
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[2]
LOCK = ROOT / "Cargo.lock"
AUDIT = ROOT / ".cargo" / "audit.toml"
DENY = ROOT / "deny.toml"

# advisory id -> (crate, minimum version, why the bump was the answer)
#
# Add a row here whenever an advisory is closed by bumping rather than by
# accepting it. A row costs one line and buys the guarantee that the next
# lockfile rewrite cannot silently undo the fix.
FLOORS: dict[str, tuple[str, str, str]] = {
    "RUSTSEC-2026-0185": (
        "quinn-proto",
        "0.11.15",
        "Remote memory exhaustion: unbounded out-of-order stream reassembly. "
        "Arrives under reqwest; cargo-deny never reported it because "
        "quinn is not in the default feature graph, so only cargo-audit "
        "(which reads the lockfile whole) saw it. Bumped on #460.",
    ),
    "RUSTSEC-2026-0190": (
        "anyhow",
        "1.0.104",
        "Unsound `Error::downcast_mut`. Recorded as fixed in deny.toml's "
        "prose since 08-09-2026; this is that sentence made checkable.",
    ),
    "RUSTSEC-2026-0204": (
        "crossbeam-epoch",
        "0.9.21",
        "Invalid pointer dereference. Same prose, same reason.",
    ),
    "RUSTSEC-2026-0049": (
        "rustls-webpki",
        "0.103.13",
        "One of four rustls-webpki advisories accepted until 08-09-2026 and "
        "then removed from both ignore lists because the tree had moved past "
        "them. Nothing recorded that, so nothing would have noticed the tree "
        "moving back. The floor is the highest of the four patched versions "
        "(RUSTSEC-2026-0104), which covers all of them.",
    ),
    "RUSTSEC-2026-0098": ("rustls-webpki", "0.103.13", "See RUSTSEC-2026-0049."),
    "RUSTSEC-2026-0099": ("rustls-webpki", "0.103.13", "See RUSTSEC-2026-0049."),
    "RUSTSEC-2026-0104": ("rustls-webpki", "0.103.13", "See RUSTSEC-2026-0049."),
}


def as_number(version: str) -> tuple[int, ...]:
    """Numeric release part of a semver string, for ordering.

    Pre-release suffixes are dropped rather than ordered: no floor here names
    one, and getting pre-release precedence subtly wrong would make this guard
    lie in the direction of passing.
    """
    core = version.split("+")[0].split("-")[0]
    return tuple(int(part) for part in core.split(".") if part.isdigit())


def below_floors(packages: list[dict], floors: dict) -> list[str]:
    """Advisory ids whose crate is present below its floor."""
    problems = []
    for advisory, (crate, floor, why) in sorted(floors.items()):
        for package in packages:
            if package.get("name") != crate:
                continue
            found = package.get("version", "0")
            if as_number(found) < as_number(floor):
                problems.append(
                    f"{crate} {found} is below {floor}, which is where "
                    f"{advisory} was fixed.\n      {why}"
                )
    return problems


def accepted_ids(path: pathlib.Path) -> set[str]:
    if not path.is_file():
        return set()
    return set(tomllib.loads(path.read_text()).get("advisories", {}).get("ignore", []))


def main() -> int:
    if not LOCK.is_file():
        print(f"FATAL: {LOCK} is missing", file=sys.stderr)
        return 2
    try:
        packages = tomllib.loads(LOCK.read_text()).get("package", [])
    except tomllib.TOMLDecodeError as exc:
        print(f"FATAL: Cargo.lock does not parse: {exc}", file=sys.stderr)
        return 2

    problems = below_floors(packages, FLOORS)

    both = sorted((accepted_ids(AUDIT) | accepted_ids(DENY)) & set(FLOORS))
    for advisory in both:
        problems.append(
            f"{advisory} is recorded here as fixed by a bump AND accepted in an "
            "ignore list. One of the two is wrong: an accepted advisory is a "
            "risk somebody signed for, a fixed one is not present."
        )

    if problems:
        print("FAIL: an advisory that was fixed by a version bump is back.\n",
              file=sys.stderr)
        for problem in problems:
            print(f"    - {problem}", file=sys.stderr)
        print(
            "\nIf the downgrade is deliberate, the advisory has to move to the "
            "ignore lists with a reason and a date -- not disappear from both "
            "registers.",
            file=sys.stderr,
        )
        return 1

    print(f"[advisory-floors] OK: {len(FLOORS)} fixed advisory/-ies still fixed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
