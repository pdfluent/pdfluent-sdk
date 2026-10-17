#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Is this exact version of this crate on crates.io? Ask crates.io (#358).

`publish_ordered.sh` used to ask `cargo info <name>@<version>` from the repo
root. Inside the workspace that resolves to the local path member and never
touches the registry, so every one of the 32 workspace crates answered "yes":

    $ cd <worktree>;  cargo info pdf-invoice@1.0.0
    version: 1.0.0 (from ./crates/pdf-invoice)
    $ cd /tmp;        cargo info pdf-invoice@1.0.0
    error: could not find `pdf-invoice@1.0.0` in registry ...

`--live` therefore skipped all 32 crates, never reached `cargo publish`, and
exited 0 saying "done". The same call was the post-publish visibility wait, so a
failed upload was still announced as visible in the index. The only automated
crates.io path in the tree published nothing, in both directions silently.

THREE ANSWERS, NOT TWO. "I could not tell" is its own exit code. Folding it into
"not published" is safe for the skip check (crates.io refuses a duplicate) and
wrong for the visibility wait, where it would confirm an upload that never
landed. The caller decides per site; this script never guesses.

Exit codes:
  0  the version is on crates.io
  1  it is not (the crate is unknown, or the version is not among its versions)
  2  could not tell -- network, HTTP status, or a body that is not the API's

`PDFLUENT_CRATES_IO_API` overrides the base URL, for the test.
"""
from __future__ import annotations

import http.client
import json
import os
import re
import sys
import urllib.error
import urllib.request

DEFAULT_API = "https://crates.io/api/v1/crates"
# crates.io asks for a user agent that says who is calling and refuses requests
# without one.
USER_AGENT = "pdfluent-release-publish (hello@pdfluent.com)"
TIMEOUT_SECONDS = 30

PRESENT, ABSENT, UNKNOWN = 0, 1, 2

# What crates.io accepts as a crate name. Anything else is a caller mistake,
# not an answer about the registry -- and pasting it into a URL is how this
# script learned to raise InvalidURL instead of reporting one.
NAME = re.compile(r"[A-Za-z0-9_-]{1,64}\Z")


def versions_of(name: str) -> list[str] | None:
    """Every version crates.io lists for `name`; None when it says nothing."""
    base = os.environ.get("PDFLUENT_CRATES_IO_API", DEFAULT_API).rstrip("/")
    request = urllib.request.Request(f"{base}/{name}",
                                     headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as answer:
            body = answer.read().decode("utf-8")
    except urllib.error.HTTPError as why:
        # 404 is an answer: crates.io says it does not have this crate. Any
        # other status is the server declining to answer, which is not the same.
        if why.code == 404:
            return []
        print(f"crates.io answered HTTP {why.code} for {name}", file=sys.stderr)
        return None
    except (urllib.error.URLError, TimeoutError, OSError,
            http.client.HTTPException, ValueError) as why:
        # ValueError covers http.client.InvalidURL. An exception that escapes
        # here would leave Python exiting 1, and 1 is "not on crates.io" --
        # a crash would quietly become an answer.
        print(f"could not reach crates.io for {name}: {why}", file=sys.stderr)
        return None

    try:
        payload = json.loads(body)
    except json.JSONDecodeError as why:
        print(f"crates.io returned a body that is not JSON for {name}: {why}",
              file=sys.stderr)
        return None

    # `[]` and `null` are valid JSON and are not an answer from this API. The
    # membership test below survives a list and `.get` does not, and an
    # AttributeError here would leave Python exiting 1 -- which this script's
    # contract reads as "not on crates.io". (codex, #1915)
    if not isinstance(payload, dict):
        print(f"crates.io returned {type(payload).__name__}, not an object, "
              f"for {name}", file=sys.stderr)
        return None
    # A 404 above is the registry saying it does not have this crate, and that
    # is an answer. An error envelope under HTTP 200 is not: it is a proxy or a
    # regression contradicting itself, sometimes while carrying versions in the
    # same body. Reading it as "absent" would try a duplicate publish and make
    # the visibility wait sit out its five minutes after an upload that
    # worked. (codex, #1915)
    if "errors" in payload:
        print(f"crates.io returned an error envelope under HTTP 200 for "
              f"{name}: {payload['errors']!r}", file=sys.stderr)
        return None
    found = payload.get("versions")
    if not isinstance(found, list):
        print(f"crates.io returned no version list for {name}", file=sys.stderr)
        return None
    # EVERY ENTRY, OR NONE OF THEM. Dropping the entries that do not look like
    # versions leaves a shorter list that still answers, and the answer is
    # "your version is not in it" -- ABSENT, out of a body this script could
    # not read. That is the three-state contract leaking again: the skip check
    # would try a duplicate and the visibility wait would spend five minutes
    # and then fail after an upload that worked. (codex, #1915)
    numbers: list[str] = []
    for entry in found:
        number = entry.get("num") if isinstance(entry, dict) else None
        if not isinstance(number, str):
            print(f"crates.io returned a version entry that is not a version "
                  f"for {name}: {entry!r}", file=sys.stderr)
            return None
        numbers.append(number)
    return numbers


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print("usage: crate_version_is_on_crates_io.py <crate> <version>",
              file=sys.stderr)
        return UNKNOWN
    name, version = argv[1], argv[2]
    if not NAME.match(name):
        print(f"not a crate name: {name!r}", file=sys.stderr)
        return UNKNOWN
    if not version.strip():
        print("no version given", file=sys.stderr)
        return UNKNOWN
    known = versions_of(name)
    if known is None:
        return UNKNOWN
    return PRESENT if version in known else ABSENT


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
