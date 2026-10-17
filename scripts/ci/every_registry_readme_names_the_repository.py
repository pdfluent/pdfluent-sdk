#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Every registry page says where the source is, at the top (#205 point 6/7).

WHY

Five registry pages -- crates.io, PyPI, npm, NuGet, Maven Central -- are where
somebody meets this project without having chosen it yet. Each renders a README
we wrote and metadata we filled in, and each is a place the way back to the
repository can quietly not be.

Measured on the live pages on 06-09-2026, before this guard existed: all five
carried the repository in their METADATA (`repository`, `project_urls`,
`<repository url>`, `<scm>`), and not one of the four that render a README named
it in the opening lines. The reader who scrolls past the first screen is not the
reader this is for.

WHAT IT CHECKS

For each channel: the README exists, and the repository URL appears within its
first few non-blank lines -- above the fold, where a link is a route and not a
footnote. And the manifest beside it names the same repository, so the two
answers cannot drift into disagreeing about which repository this is.

MAVEN IS DELIBERATELY NOT IN THE README HALF. Maven Central renders no README:
its page is built from the POM. So the POM's `<scm>` and `<url>` are the whole
of what a visitor there can be told, and that is what is checked for it.

# FLOOR: at least 4 channels. A register that has quietly emptied approves
# every channel in it, which is none.
"""
from __future__ import annotations

import json
import pathlib
import re
import sys

REPO = "https://github.com/pdfluent/pdfluent-sdk"
ROOT = pathlib.Path(__file__).resolve().parents[2]

# How far into a README still counts as "the reader has not scrolled". Six
# non-blank lines is the title, a tagline, a badge row and the link.
ABOVE_THE_FOLD = 6
MINIMUM_CHANNELS = 4

# channel -> (README path or None, manifest path, how to read the repo out of it)
CHANNELS: dict[str, tuple[str | None, str]] = {
    "crates.io":     ("crates/pdfluent/README.md",              "crates/pdfluent/Cargo.toml"),
    "PyPI":          ("crates/pdf-python/README.md",            "crates/pdf-python/pyproject.toml"),
    "npm":           ("crates/pdf-node/README.md",              "crates/pdf-node/package.json"),
    "NuGet":         ("bindings/dotnet/src/PDFluent/README.md", "bindings/dotnet/src/PDFluent/PDFluent.csproj"),
    "Maven Central": (None,                                     "bindings/java/pom.xml"),
}


def above_the_fold(text: str, lines: int = ABOVE_THE_FOLD) -> list[str]:
    return [l for l in text.splitlines() if l.strip()][:lines]


def judge(root: pathlib.Path = ROOT, channels: dict | None = None,
          repo: str = REPO) -> list[str]:
    """Every complaint about one tree. Empty when all five point home."""
    # The floor is about THIS register, not about a caller's fixture: a test
    # that plants one broken channel is not a register somebody emptied, and a
    # floor that fired on it would drown the finding it was checking for.
    shipped = channels is None
    channels = CHANNELS if shipped else channels
    problems: list[str] = []

    if shipped and len(channels) < MINIMUM_CHANNELS:
        problems.append(
            f"the register holds {len(channels)} channel(s), below the floor of "
            f"{MINIMUM_CHANNELS}: it has been emptied rather than satisfied")

    for channel, (readme, manifest) in channels.items():
        if readme is not None:
            path = root / readme
            if not path.is_file():
                problems.append(f"{channel}: {readme} is missing, so the page it "
                                f"renders says nothing about where the source is")
            else:
                top = above_the_fold(path.read_text(encoding="utf-8", errors="replace"))
                if not any(repo in line for line in top):
                    problems.append(
                        f"{channel}: {readme} does not name {repo} in its first "
                        f"{ABOVE_THE_FOLD} lines -- the link exists further down "
                        f"or not at all, and the visitor this is for has not "
                        f"scrolled")

        mpath = root / manifest
        if not mpath.is_file():
            problems.append(f"{channel}: {manifest} is missing")
            continue
        text = mpath.read_text(encoding="utf-8", errors="replace")
        if manifest.endswith(".json"):
            # npm spells it as an object with a `git+…` url.
            found = repo in json.dumps(json.loads(text))
        else:
            found = repo in text
        if not found:
            problems.append(
                f"{channel}: {manifest} does not name {repo}, so the registry "
                f"page's own metadata points somewhere else than its README")

    return problems


def main() -> int:
    problems = judge()
    if not problems:
        print(f"[registry-readme] OK: {len(CHANNELS)} channel(s) name "
              f"{REPO} in their metadata, and the {len([c for c in CHANNELS.values() if c[0]])} "
              f"that render a README name it above the fold.")
        return 0
    for p in problems:
        print(f"[registry-readme] FATAL: {p}", file=sys.stderr)
    print(f"[registry-readme] {len(problems)} problem(s). A registry page is "
          f"where somebody meets this project without having chosen it; the way "
          f"back to the source belongs where they are still looking.",
          file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
