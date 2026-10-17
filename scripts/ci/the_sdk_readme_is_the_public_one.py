#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""`README.md` here and on `pdfluent/pdfluent-sdk` are the same file (#637).

WHAT WENT WRONG WITHOUT IT

The SDK README exists twice: in this tree, and as the front page of the public
repository. #231 made the public repository the place people work in, so a
change can land on either side -- a pull request from outside lands there, a
guard-driven fix lands here. By 29-09-2026 the two had drifted both ways. This
side had the hero image, a licence paragraph that described the build
correctly, and had dropped an unregistered "90+ built-ins"; the public side
still told readers the build refuses features without a key, and had the
Discussions link and the Community section this side lacked. Each copy was
right where the other was wrong, and nothing compared them.

WHAT IT CHECKS

The two files are byte-identical. There is no allowed difference: a README that
may differ "a little" is two READMEs again. A change to one lands in both, in
the same afternoon.

The public file is fetched from `raw.githubusercontent.com`. A fetch that fails
is FATAL, not a skip: a comparison that did not happen must not read as one that
passed. `--remote <file>` replaces the fetch with a local file; that is what the
test uses, so the test needs no network.

# FLOOR: both files must hold >= 40 lines. Two empty files are identical, and a
# comparison of nothing reports the same green as a comparison of the real page.
"""

from __future__ import annotations

import argparse
import difflib
import pathlib
import sys
import urllib.error
import urllib.request

REPO = pathlib.Path(__file__).resolve().parent.parent.parent
README = REPO / "README.md"
PUBLIC_URL = "https://raw.githubusercontent.com/pdfluent/pdfluent-sdk/main/README.md"
MIN_LINES = 40


def fetch(url: str) -> str:
    with urllib.request.urlopen(url, timeout=30) as response:
        return response.read().decode("utf-8")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--remote", type=pathlib.Path,
                        help="compare against this file instead of fetching the public README")
    args = parser.parse_args(argv)

    if not README.is_file():
        print(f"[sdk-readme] FATAL: {README} is missing, and that is not a pass",
              file=sys.stderr)
        return 1
    local = README.read_text(encoding="utf-8")

    if args.remote is not None:
        if not args.remote.is_file():
            print(f"[sdk-readme] FATAL: --remote {args.remote} does not exist",
                  file=sys.stderr)
            return 1
        public, source = args.remote.read_text(encoding="utf-8"), str(args.remote)
    else:
        try:
            public, source = fetch(PUBLIC_URL), PUBLIC_URL
        except (urllib.error.URLError, OSError, UnicodeDecodeError) as exc:
            print(f"[sdk-readme] FATAL: could not fetch {PUBLIC_URL}: {exc}. The "
                  "comparison did not happen, so this is a failure, not a pass.",
                  file=sys.stderr)
            return 1

    for name, text in (("README.md", local), (source, public)):
        n = len(text.splitlines())
        if n < MIN_LINES:
            print(f"[sdk-readme] FATAL: {name} is {n} line(s), below the floor of "
                  f"{MIN_LINES}. Comparing a page that short would report on nothing.",
                  file=sys.stderr)
            return 1

    if local != public:
        diff = list(difflib.unified_diff(
            public.splitlines(keepends=True), local.splitlines(keepends=True),
            fromfile="pdfluent-sdk:README.md", tofile="engine:README.md", n=1))
        print("[sdk-readme] README.md here differs from the public one "
              f"({sum(1 for l in diff if l[:1] in '+-') - 2} changed line(s)):",
              file=sys.stderr)
        sys.stderr.writelines(diff[:80])
        if len(diff) > 80:
            print(f"  ... {len(diff) - 80} more diff line(s)", file=sys.stderr)
        print("\n  Make the same change on the other side: a pull request on "
              "pdfluent/pdfluent-sdk, or a commit here. #637 has why there is "
              "one README and not two.", file=sys.stderr)
        return 1

    print(f"[sdk-readme] OK: README.md is identical to {source} "
          f"({len(local.splitlines())} lines).")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
