#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository.
"""The fork-README guard still bites on each of the ways it can be broken.

A guard over nine files that happen to be right is indistinguishable from a
guard that returns zero faults for everything. So each rule is driven with a
README that breaks exactly it, and with one that breaks nothing -- and the
commercial-claim rule is driven twice, once with a permissive manifest and once
with ours, because the failure it looks for is the DISAGREEMENT and not the
words.

The blockquote case is here for a reason rather than for completeness:
`every_fork_names_its_upstream.py` writes the fork notice wrapped, so the
sentence this guard looks for arrives split across two lines behind `> `. Read
the bytes and every crate that carries the generated notice reads as missing a
recommendation it does carry.

Pure text, no filesystem and no network: this runs in the commit hook.
"""
from __future__ import annotations

import importlib.util
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent

GOOD = """# pdf-render

A rasteriser.

This crate is a **fork of [`hayro`](https://github.com/LaurenzV/hayro)** by
Laurenz Stampfl, substantially extended.

If you do not need PDFluent's changes, upstream
[`hayro`](https://github.com/LaurenzV/hayro) is the better choice: it is the
original.

## License

Apache-2.0 OR MIT.
"""

# The generated fork notice, as `every_fork_names_its_upstream.py --write` puts
# it in: a blockquote, with the recommendation wrapped mid-phrase.
GENERATED_NOTICE = """# pdf-render

> **This crate is a fork.** It began as [`hayro`](https://github.com/LaurenzV/hayro) at
> version 0.5.0 and is maintained separately here. Upstream is actively developed
> and is the better
> choice if you do not need the changes made for PDFluent.

A rasteriser.

## License

Apache-2.0 OR MIT.
"""

NO_RECOMMENDATION = GOOD.replace(
    """If you do not need PDFluent's changes, upstream
[`hayro`](https://github.com/LaurenzV/hayro) is the better choice: it is the
original.

""", "")

# The recommendation points somewhere else -- our own crates.io page, say --
# which reads as a recommendation and sends nobody upstream.
RECOMMENDATION_POINTS_HOME = GOOD.replace(
    "[`hayro`](https://github.com/LaurenzV/hayro) is the better choice: it is the\noriginal.",
    "[`pdf-render`](https://crates.io/crates/pdf-render) is the better choice.")

COMMERCIAL = GOOD.replace(
    "## License\n\nApache-2.0 OR MIT.",
    "## Licensing\n\nFree for evaluation. Production use requires a valid "
    "PDFluent commercial license, and PDFluent extensions are governed by the "
    "PDFluent Commercial License.")


def load():
    spec = importlib.util.spec_from_file_location(
        "fork_readme", HERE / "a_forked_readme_sends_you_upstream.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main() -> int:
    guard = load()
    broken: list[str] = []

    def faults(readme: str, licence: str | None = "Apache-2.0 OR MIT") -> list[str]:
        return guard.faults("pdf-render", "hayro", readme, licence)

    def must_pass(name: str, readme: str, licence: str | None = "Apache-2.0 OR MIT"):
        found = faults(readme, licence)
        if found:
            broken.append(f"{name} should be accepted, and is not: {found}")

    def must_fail(name: str, readme: str, needle: str,
                  licence: str | None = "Apache-2.0 OR MIT"):
        found = faults(readme, licence)
        if not any(needle in f for f in found):
            broken.append(f"{name} passes the guard; expected a fault mentioning "
                          f"{needle!r}, got {found}")

    must_pass("a README that does everything", GOOD)
    must_pass("the generated fork notice, wrapped mid-phrase", GENERATED_NOTICE)
    must_fail("a README with no recommendation", NO_RECOMMENDATION,
              "no paragraph calls upstream")
    must_fail("a recommendation pointing at ourselves", RECOMMENDATION_POINTS_HOME,
              "paragraph does not link")
    must_fail("commercial terms on a permissive crate", COMMERCIAL,
              "PDFluent Commercial Licence as governing this crate")

    # The same README under our own licence is not this guard's business: the
    # fault is the disagreement with the manifest, not the words.
    ours = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"
    if any("Commercial" in f for f in faults(COMMERCIAL, ours)):
        broken.append("commercial terms are reported for a crate whose manifest "
                      "declares them, which makes the rule about words rather "
                      "than about the disagreement")

    # An unknown upstream must fail rather than be skipped: a fork added to the
    # register with no URL here is exactly the crate nobody checked.
    if not guard.faults("pdf-render", "some-new-upstream", GOOD, "MIT"):
        broken.append("an upstream with no recorded URL passes silently")

    # And the floor has to be a floor: a register that shrinks below it is a
    # register that stopped describing the tree.
    if guard.FLOOR < 2:
        broken.append(f"FLOOR is {guard.FLOOR}, low enough that an empty register "
                      "reads as clean")

    if broken:
        for line in broken:
            print(f"FAIL: {line}", file=sys.stderr)
        return 1
    print("[fork-readme-test] OK: each rule refuses the README that breaks it, "
          "accepts the one that does not, and accepts the generated notice.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
