#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Refuse a wheel whose platform tag PyPI will not accept, before the upload.

WHY THIS EXISTS

On 10-09-2026 the Linux wheel for 1.0.0 was built on a fresh Ubuntu 26.04
machine. It was a correct wheel: the metadata was right, both licence files
were inside, and `twine check` said PASSED. PyPI would have refused it at the
door, because glibc 2.43 gave maturin no compatible platform tag and it fell
back to `linux_x86_64` -- a tag the index does not accept. `twine check` reads
metadata; it says nothing at all about the tag, so the green check was not
about the thing that was wrong.

WHY IT PARSES THE TAG INSTEAD OF MATCHING THE NAME

The first version of this rule lived in the runbook as a `case` over the file
name: `*manylinux*|*macosx*|*win_amd64*` and so on. A substring is not a tag.
`pdfluent-1.0.0-cp38-abi3-macosx_11_0_arm64.linux_x86_64.whl` contains
`macosx` and is not publishable; a distribution named `macosx-tools` would pass
whatever it carried. So the file name is split on PEP 427 fields here and only
the last one -- the compressed tag set of platforms -- decides, with EVERY
platform in that set having to be acceptable, because the index refuses the
file, not one of its tags.

WHY AN EMPTY DIRECTORY IS AN ERROR, AND SO IS A NAME WITH NO FILE

The `case` loop in the runbook passed a directory with no wheels in it, which
is the shape a release has when a build silently failed. Nothing to refuse is
not the same as nothing to refuse it for, so zero wheels is exit 1 -- and so is
a wheel named on the command line that is not on the disk, which is the same
failure wearing the right name.

USAGE

    python3 scripts/release/wheel_platform_tag_is_publishable.py dist/
    python3 scripts/release/wheel_platform_tag_is_publishable.py dist/*.whl

Exit codes:
  0  every wheel carries a platform tag PyPI accepts
  1  a wheel does not, no wheel was given, or a name is not a wheel name
"""
from __future__ import annotations

import pathlib
import re
import sys

# What warehouse accepts, in the shape it accepts it. Sources: PEP 425 (win),
# PEP 600 (manylinux_x_y), PEP 656 (musllinux_x_y), PEP 513/571/599 (the three
# legacy manylinux aliases) and PEP 427 (`any`).
ACCEPTED = (
    re.compile(r"^any$"),
    re.compile(r"^macosx_\d+_\d+_(?:x86_64|arm64|universal2|universal|intel|fat64|fat32|ppc64|ppc)$"),
    re.compile(r"^manylinux1_(?:x86_64|i686)$"),
    re.compile(r"^manylinux2010_(?:x86_64|i686)$"),
    re.compile(r"^manylinux2014_(?:x86_64|i686|aarch64|armv7l|ppc64|ppc64le|s390x)$"),
    re.compile(r"^manylinux_\d+_\d+_(?:x86_64|i686|aarch64|armv7l|ppc64|ppc64le|s390x|riscv64)$"),
    re.compile(r"^musllinux_\d+_\d+_(?:x86_64|i686|aarch64|armv7l|ppc64le|s390x|riscv64)$"),
    re.compile(r"^win32$"),
    re.compile(r"^win_amd64$"),
    re.compile(r"^win_arm64$"),
)

# PEP 427: {distribution}-{version}(-{build tag})?-{python tag}-{abi tag}-{platform tag}
WHEEL_NAME = re.compile(r"^(?P<rest>.+?)-(?P<python>[^-]+)-(?P<abi>[^-]+)-(?P<platform>[^-]+)\.whl$")


def platforms(name: str) -> list[str]:
    """The platform tags in a wheel file name, or [] if it is not a wheel name."""
    match = WHEEL_NAME.match(name)
    if match is None:
        return []
    fields = match.group("rest").split("-")
    # distribution, version and an optional build tag: two or three fields, and
    # a name with one field is a name missing its version rather than a wheel.
    if len(fields) not in (2, 3) or not all(fields):
        return []
    return match.group("platform").split(".")


def unacceptable(name: str) -> list[str]:
    """The platform tags in `name` that PyPI will not take."""
    return [tag for tag in platforms(name) if not any(ok.match(tag) for ok in ACCEPTED)]


def wheels(arguments: list[str]) -> list[pathlib.Path]:
    """Every wheel named by the arguments; a directory contributes its own."""
    found: list[pathlib.Path] = []
    for argument in arguments:
        path = pathlib.Path(argument)
        if path.is_dir():
            found.extend(sorted(path.glob("*.whl")))
        else:
            found.append(path)
    return found


def main(argv: list[str]) -> int:
    paths = wheels(argv[1:])
    if not paths:
        print(
            "REFUSED: no wheel was given. A directory with no wheel in it is what "
            "a release looks like when the build failed quietly.",
            file=sys.stderr,
        )
        return 1

    problems = 0
    for path in paths:
        if not path.is_file():
            print(f"REFUSED: {path} does not exist", file=sys.stderr)
            problems += 1
            continue
        tags = platforms(path.name)
        if not tags:
            print(f"REFUSED: {path} is not a wheel file name (PEP 427)", file=sys.stderr)
            problems += 1
            continue
        bad = unacceptable(path.name)
        if bad:
            print(
                f"REFUSED: {path} carries the platform tag "
                f"{', '.join(bad)}, which PyPI does not accept",
                file=sys.stderr,
            )
            problems += 1
        else:
            print(f"ok: {path.name} -> {', '.join(tags)}")

    if problems:
        print(f"{problems} of {len(paths)} wheels would not survive the upload", file=sys.stderr)
        return 1
    print(f"{len(paths)} wheels carry a platform tag PyPI accepts")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
