#!/usr/bin/env python3
"""Every channel we publish states its licence in its own manifest (#300, #304).

license_gate.py compares a manifest against docs/LICENSE_POLICY.toml only when
the manifest carries a real SPDX expression:

    if gedeclareerd and not NIET_SPDX.match(gedeclareerd) and gedeclareerd != verwacht

`SEE LICENSE IN LICENSE` matches NIET_SPDX, so the comparison is SKIPPED and the
policy entry is never checked against anything. Measured on the #257 flip branch
(ab77d318): of the four own_packages strings the flip edits, exactly ONE is
covered -- reverting crates/xfa-wasm/Cargo.toml turns license_gate red, and
reverting the other three leaves every licence gate green.

  crates/pdf-node/package.json        declares "SEE LICENSE IN LICENSE", so the
                                      policy value is never compared
  bindings/dotnet/.../PDFluent.csproj carries PackageLicenseFile, no
                                      PackageLicenseExpression -- nothing to
                                      compare
  crates/pdf-python/pyproject.toml    the scanner reads runtime dependencies and
                                      never the project's own declaration

So the flip's proof -- "the gates are green afterwards" -- is available for one
channel out of four. This guard makes the other three measurable: where the
policy names one of our manifests, that manifest must say so itself, in the
spelling its own ecosystem understands.

A dual expression is fine. What is not fine is a manifest that declines to say
anything, because then the policy is the only place the licence exists and no
gate can catch it drifting.
"""
from __future__ import annotations
import json, pathlib, re, sys, tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
POLICY = REPO / "docs" / "LICENSE_POLICY.toml"

# Not an SPDX expression: npm's file pointer, and the "no licence" markers.
ESCAPE_HATCH = re.compile(r"^\s*(SEE LICENSE IN|UNLICENSED)", re.I)
SPLIT = re.compile(r"\s+(?:OR|AND)\s+", re.I)


def operands(expr: str) -> set[str]:
    return {p.strip("() ") for p in SPLIT.split(expr.strip()) if p.strip("() ")}


def npm_licence(path: pathlib.Path) -> str | None:
    return (json.loads(path.read_text()) or {}).get("license")


def cargo_licence(path: pathlib.Path) -> str | None:
    d = tomllib.loads(path.read_text())
    return (d.get("package") or {}).get("license")


def csproj_licence(path: pathlib.Path) -> str | None:
    m = re.search(r"<PackageLicenseExpression>\s*([^<]+?)\s*</PackageLicenseExpression>",
                  path.read_text())
    return m.group(1) if m else None


def pyproject_licence(path: pathlib.Path) -> str | None:
    d = tomllib.loads(path.read_text())
    lic = (d.get("project") or {}).get("license")
    if isinstance(lic, str):
        return lic
    if isinstance(lic, dict):
        return lic.get("text")
    return None


READERS = {
    ".json": (npm_licence, "the `license` field"),
    ".toml": (None, ""),  # decided by filename below
    ".csproj": (csproj_licence, "<PackageLicenseExpression>"),
}


def read_declaration(rel: str) -> tuple[str | None, str]:
    path = REPO / rel
    if not path.is_file():
        return None, "the file is missing"
    if path.name == "pyproject.toml":
        return pyproject_licence(path), "[project] license"
    if path.suffix == ".toml":
        return cargo_licence(path), "[package] license"
    reader, where = READERS.get(path.suffix, (None, ""))
    if reader is None:
        return None, "no reader for this manifest type"
    return reader(path), where


def main() -> int:
    if not POLICY.is_file():
        print(f"[channels] FATAL: {POLICY} is missing.", file=sys.stderr)
        return 2
    pol = tomllib.loads(POLICY.read_text())
    own = pol.get("own_packages") or {}
    named = {k: v for k, v in own.items() if isinstance(v, str)}
    if not named:
        print("[channels] FATAL: own_packages names no manifest at all. Refusing "
              "to report a clean result for a scan with nothing to scan.",
              file=sys.stderr)
        return 2

    problems: list[str] = []
    ok_count = 0
    # One canonical expression across all four channels, so the four registers
    # are checkable against a single constant instead of against each other.
    # A channel may spell it as its ecosystem requires, but the string is one.
    canonical = {v for v in named.values()}
    if len(canonical) > 1:
        problems_pre = ", ".join(sorted(canonical))
        print(f"[channels] FAIL: own_packages holds {len(canonical)} different "
              f"expressions ({problems_pre}). The point of a canonical licence is "
              "that four registers can be checked against one constant; with "
              "several, each channel can only be checked against itself.",
              file=sys.stderr)
        return 1

    for rel, expected in sorted(named.items()):
        declared, where = read_declaration(rel)
        if declared is None:
            problems.append(
                f"{rel}: the policy says {expected!r}, and the manifest declares "
                f"nothing ({where}). The policy is then the only place this "
                "licence exists, so nothing can catch it drifting.")
            continue
        if ESCAPE_HATCH.match(declared):
            problems.append(
                f"{rel}: declares {declared!r}, which is not an SPDX expression, "
                f"so license_gate.py skips the comparison against the policy's "
                f"{expected!r} entirely. The entry reads as coverage and is not.")
            continue
        # Operand sets, not substring or equality: the canonical value is itself
        # a compound ("A OR B"), so asking whether it is one operand of the
        # declaration can never be true. What has to hold is that everything the
        # policy requires is present -- a manifest may say more (a wider dual),
        # never less.
        missing = operands(expected) - operands(declared)
        if missing:
            problems.append(
                f"{rel}: declares {declared!r}, which is missing "
                f"{', '.join(sorted(missing))} from the policy's {expected!r}. "
                "One of the two is out of date.")
            continue
        ok_count += 1

    if problems:
        print(f"[channels] FAIL: {len(problems)} of {len(named)} channel(s) do not "
              "state their own licence:", file=sys.stderr)
        for p in problems:
            print(f"    {p}", file=sys.stderr)
        print("\n  Where the policy names one of our manifests, that manifest has "
              "to say so\n  itself. Otherwise a licence change is provable for the "
              "channels that\n  declare, and unfalsifiable for the ones that do "
              "not.", file=sys.stderr)
        return 1

    print(f"[channels] OK: {ok_count} channel(s) named in own_packages, each "
          "declaring its licence in its own manifest.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
