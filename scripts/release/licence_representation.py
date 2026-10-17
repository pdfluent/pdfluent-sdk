#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Does a built package carry the licence representation the policy declares?

audit-all-packages.sh used to grep the package's LICENSE for the string
"PDFluent Commercial License". Since #349 LICENSE is the AGPL text and nothing
else, and the commercial text spells "Licence", so the grep failed on every
correct wasm package of 1.1.0. The Python channel grepped the same file for
"PDFluent Commercial License|MIT|Apache" and only warned. Neither check read
the policy, so neither could say what the right answer was.

This script reads docs/LICENSE_POLICY.toml and judges the package against it:

  npm     the channel's [channel_representation] entry. package.json `license`
          must equal `publishes` exactly; `publishes` is a pointer
          (`SEE LICENSE IN <file>`), so <file> must be in the package, must be
          the file the policy names, must open with
          `SPDX-License-Identifier: <canonical>`, and must be listed in
          package.json `files` when that list exists (npm packs nothing else).
  texts   the licence texts the canonical expression names, for a channel that
          states the expression in its own metadata (the wheel).

Both modes then require every licence text the canonical expression names:
AGPL-3.0-only is LICENSE with the AGPL text, LicenseRef-PDFluent-Commercial is
LICENSE-COMMERCIAL with the commercial text. A term without a known text is an
error, so a change to the canonical expression has to come through here.

Exit codes: 0 sound, 1 a finding (each printed on stderr), 2 usage or policy
error.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
POLICY = REPO / "docs" / "LICENSE_POLICY.toml"

# The text each term of the canonical expression stands for in a package:
# (file name, a line the text must contain).
TEXTS: dict[str, tuple[str, str]] = {
    "AGPL-3.0-only": ("LICENSE", "GNU AFFERO GENERAL PUBLIC LICENSE"),
    "LicenseRef-PDFluent-Commercial": ("LICENSE-COMMERCIAL", "PDFluent Commercial Licence"),
}

POINTER = re.compile(r"^SEE LICENSE IN (\S+)$")


class PolicyError(Exception):
    pass


def load_policy(path: pathlib.Path = POLICY) -> dict:
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as exc:
        raise PolicyError(f"cannot read {path}: {exc}") from exc


def canonical_expression(policy: dict) -> str:
    """The one canonical expression every representation entry stands for."""
    found = {w.get("canonical") for w in policy.get("channel_representation", {}).values()}
    if len(found) != 1 or None in found:
        raise PolicyError(
            f"[channel_representation] does not agree on one canonical expression: {sorted(map(str, found))}"
        )
    return found.pop()


def _find(pkg_dir: pathlib.Path, name: str, *, anywhere: bool) -> pathlib.Path | None:
    if not anywhere:
        p = pkg_dir / name
        return p if p.is_file() else None
    hits = sorted(p for p in pkg_dir.rglob(name) if p.is_file())
    return hits[0] if hits else None


def check_texts(pkg_dir: pathlib.Path, canonical: str, *, anywhere: bool) -> list[str]:
    findings = []
    for term in canonical.split(" OR "):
        if term not in TEXTS:
            raise PolicyError(
                f"canonical term {term!r} has no known licence text; add it to TEXTS in {pathlib.Path(__file__).name}"
            )
        name, marker = TEXTS[term]
        path = _find(pkg_dir, name, anywhere=anywhere)
        if path is None:
            findings.append(f"{name} is missing; {term} in {canonical!r} names it")
        elif marker not in path.read_text(encoding="utf-8", errors="replace"):
            findings.append(f"{name} does not carry the {term} text (no {marker!r})")
    return findings


def check_npm(pkg_dir: pathlib.Path, entry: str, policy: dict) -> list[str]:
    rep = policy.get("channel_representation", {}).get(entry)
    if rep is None:
        raise PolicyError(f"docs/LICENSE_POLICY.toml has no [channel_representation] entry {entry!r}")
    publishes, canonical, policy_file = rep["publishes"], rep["canonical"], rep["file"]
    findings = []

    manifest = pkg_dir / "package.json"
    try:
        data = json.loads(manifest.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        return [f"cannot read {manifest}: {exc}"]
    have = data.get("license")
    if have != publishes:
        findings.append(f"package.json license is {have!r}; the policy publishes {publishes!r} for {entry}")

    m = POINTER.match(publishes)
    if m is None:
        raise PolicyError(f"{entry}: publishes {publishes!r} is not a `SEE LICENSE IN <file>` pointer")
    target = m.group(1)
    if target != pathlib.PurePosixPath(policy_file).name:
        raise PolicyError(f"{entry}: publishes points at {target!r} but the policy file is {policy_file!r}")

    offer = pkg_dir / target
    if not offer.is_file():
        findings.append(f"{target} is missing; package.json points at it")
    else:
        lines = offer.read_text(encoding="utf-8", errors="replace").lstrip("﻿").splitlines()
        first = lines[0].rstrip() if lines else ""
        want = f"SPDX-License-Identifier: {canonical}"
        if first != want:
            findings.append(f"{target} line 1 is {first!r}; expected {want!r}")

    files = data.get("files")
    if isinstance(files, list):
        need = [target] + [TEXTS[t][0] for t in canonical.split(" OR ") if t in TEXTS]
        absent = [n for n in need if n not in files]
        if absent:
            findings.append(f"package.json files omits {', '.join(absent)}; npm would not pack them")

    findings += check_texts(pkg_dir, canonical, anywhere=False)
    return findings


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("mode", choices=["npm", "texts"])
    p.add_argument("--pkg-dir", required=True)
    p.add_argument("--entry", help="npm: the [channel_representation] key of this channel")
    p.add_argument("--policy", default=str(POLICY))
    args = p.parse_args(argv[1:])

    pkg_dir = pathlib.Path(args.pkg_dir)
    if not pkg_dir.is_dir():
        print(f"licence_representation: not a directory: {pkg_dir}", file=sys.stderr)
        return 2
    try:
        policy = load_policy(pathlib.Path(args.policy))
        if args.mode == "npm":
            if not args.entry:
                print("licence_representation: npm mode needs --entry", file=sys.stderr)
                return 2
            findings = check_npm(pkg_dir, args.entry, policy)
        else:
            findings = check_texts(pkg_dir, canonical_expression(policy), anywhere=True)
    except PolicyError as exc:
        print(f"licence_representation: {exc}", file=sys.stderr)
        return 2

    for f in findings:
        print(f"licence_representation: {f}", file=sys.stderr)
    if findings:
        return 1
    print(f"licence_representation: {args.mode} representation matches docs/LICENSE_POLICY.toml")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
