#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The order the workspace crates have to reach crates.io in (#358).

Every intra-workspace dependency is pinned with `=<version>`, so a crate can
only be packaged -- let alone published -- once the exact versions it names are
on the index. That makes a crates.io release one ordered chain rather than
thirty-two independent uploads, and getting the order wrong does not fail
early: it fails halfway, with some of the release public and immutable and the
rest of it refused.

Prints one `<name> <version>` per line, dependencies before dependents. Order
among independent crates is alphabetical, so two runs of the same tree print
the same list.

Usage:
    python3 scripts/release/topological_publish_order.py
    python3 scripts/release/topological_publish_order.py --unpublished
        also asks crates.io which of them are not there yet (needs the network)
    python3 scripts/release/topological_publish_order.py --for CRATE
        only CRATE and its transitive workspace dependencies, in publish order
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import urllib.error
import urllib.request

USER_AGENT = "pdfluent-release (team@pdfluent.com)"


def workspace() -> dict[str, dict]:
    """Every workspace package that is meant to be published."""
    out = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        capture_output=True, text=True, check=True).stdout
    return {p["name"]: p for p in json.loads(out)["packages"] if p.get("publish") != []}


def order(packages: dict[str, dict]) -> list[str]:
    """Dependencies first. Build-dependencies count; dev-dependencies do not.

    A dev-dependency is not in the published dependency graph -- cargo strips
    it from the uploaded manifest -- and counting it would invent a cycle
    between two crates that test against each other.
    """
    names = set(packages)
    needs = {
        name: {d["name"] for d in package["dependencies"]
               if d["name"] in names and d["kind"] in (None, "build")}
        for name, package in packages.items()
    }
    ordered: list[str] = []
    state: dict[str, str] = {}

    def visit(name: str, path: tuple[str, ...]) -> None:
        if state.get(name) == "done":
            return
        if state.get(name) == "open":
            raise ValueError("dependency cycle: " + " -> ".join(path + (name,)))
        state[name] = "open"
        for dependency in sorted(needs[name]):
            visit(dependency, path + (name,))
        state[name] = "done"
        ordered.append(name)

    for name in sorted(names):
        visit(name, ())
    return ordered


def closure(packages: dict[str, dict], name: str) -> list[str]:
    """The crate and its transitive workspace dependencies, in publish order.

    The same edge rule as `order`: normal and build dependencies count,
    dev-dependencies do not. The crate itself is last. An unknown crate --
    or one that is not publishable, which `workspace` excludes -- is a
    KeyError naming it.

    `package_with_workspace_deps.sh` selects exactly this set: cargo 1.94
    resolves the unpublished workspace versions locally when the crates are
    packaged together, so a pre-publication `cargo package` of the crate
    alone fails where one of its closure succeeds (#636).
    """
    if name not in packages:
        raise KeyError(f"unknown publishable workspace crate: {name}")
    names = set(packages)
    needs = {
        candidate: {d["name"] for d in package["dependencies"]
                    if d["name"] in names and d["kind"] in (None, "build")}
        for candidate, package in packages.items()
    }
    seen = {name}
    stack = [name]
    while stack:
        for dependency in needs[stack.pop()]:
            if dependency not in seen:
                seen.add(dependency)
                stack.append(dependency)
    return [candidate for candidate in order(packages) if candidate in seen]


def already_on_crates_io(name: str, version: str) -> bool:
    request = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{name}", headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            body = json.load(response)
    except urllib.error.HTTPError as why:
        if why.code == 404:
            return False
        raise
    return version in {v["num"] for v in body["versions"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--unpublished", action="store_true",
                        help="mark the crates crates.io does not have yet")
    parser.add_argument("--for", dest="for_crate", metavar="CRATE",
                        help="only CRATE and its transitive workspace dependencies")
    arguments = parser.parse_args()

    packages = workspace()
    try:
        names = (closure(packages, arguments.for_crate) if arguments.for_crate
                 is not None else order(packages))
    except KeyError as why:
        print(f"topological_publish_order: {why.args[0]}", file=sys.stderr)
        return 2
    for name in names:
        version = packages[name]["version"]
        if not arguments.unpublished:
            print(f"{name} {version}")
            continue
        state = "on crates.io" if already_on_crates_io(name, version) else "TO PUBLISH"
        print(f"{name} {version} {state}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
