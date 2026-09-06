#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""`one_offer_in_every_document.py` still goes red where it must.

A guard over prose is the easiest kind to break by accident: tighten a regex,
rewrap a paragraph, and it quietly matches nothing while still printing a green
line. That is not a hypothetical -- it is how the retired prices survived four
cleanups of the website.

So each case below copies the real documents into a scratch tree, plants one
regression that actually happened or plausibly will, and requires a non-zero
exit naming it. The unmutated copy is checked first: without that, a guard that
fails on everything would pass this file.

Usage:
    python3 scripts/ci/test_one_offer_in_every_document.py
"""
from __future__ import annotations

import pathlib
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = "scripts/ci/one_offer_in_every_document.py"
OFFER = "docs/licensing/offer.toml"
LICENCE = "LICENSE-COMMERCIAL"
DOCS = "docs/licensing.md"
FORM = "docs/licensing/order-form.md"
NOTICE = "NOTICE"

FILES = (GUARD, OFFER, LICENCE, DOCS, FORM, NOTICE)

results: list[tuple[bool, str, str]] = []


def check(what: str, ok: bool, detail: str = "") -> None:
    results.append((ok, what, detail))


def build(base: pathlib.Path) -> pathlib.Path:
    """A scratch repository holding only what the guard reads."""
    root = base / "repo"
    for rel in FILES:
        target = root / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(REPO / rel, target)
    return root


def rewrite(rel: str, old: str, new: str, count: int = 1):
    def mutate(root: pathlib.Path) -> None:
        path = root / rel
        text = path.read_text(encoding="utf-8")
        assert text.count(old) == count, f"{rel}: {old!r} occurs {text.count(old)}x, not {count}x"
        path.write_text(text.replace(old, new), encoding="utf-8")

    return mutate


def run(base: pathlib.Path, name: str, mutate) -> subprocess.CompletedProcess:
    root = build(base / name)
    if mutate:
        mutate(root)
    return subprocess.run(
        [sys.executable, str(root / GUARD)], capture_output=True, text=True,
    )


def red(what: str, result: subprocess.CompletedProcess, *must_name: str) -> None:
    check(f"{what} -> red", result.returncode != 0, result.stdout[-300:])
    for token in must_name:
        check(f"{what} -> names {token!r}", token in result.stderr, result.stderr[-300:])


def main() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        base = pathlib.Path(tmp)

        # 0. the untouched pair passes, or every case below proves nothing.
        clean = run(base, "clean", None)
        check("the unmutated documents pass", clean.returncode == 0, clean.stderr[-400:])

        # 1. the failure this issue was opened about: a price moves in one
        #    document and stays put in the others.
        red(
            "a price changes in the licence only",
            run(base, "price-drift", rewrite(LICENCE, "Commercial      999", "Commercial    1,299")),
            "does not state 999",
        )

        # 2. the direction that costs a customer money rather than us: a stale
        #    price from the retired model surviving a cleanup.
        red(
            "a retired price left in the docs",
            run(base, "retired-price", rewrite(DOCS, "| Commercial | € 999 / year", "| Commercial | € 699 / year", 1)),
            "retired on",
        )

        # 3. the sentence a licensee is judged on, paraphrased. Every word of it
        #    is load-bearing: "provides" is not "uses", and "customers pay you"
        #    is not "you charge".
        red(
            "the OEM rule paraphrased",
            run(base, "rule-paraphrase",
                rewrite(DOCS, "> conversion API, a PDF/A service, an OCR service, hosted PDF tooling — that is",
                        "> conversion API or similar — that is")),
            "verbatim",
        )

        # 4. the word that describes the previous model, applied to this one.
        red(
            "a licence sold today called perpetual",
            run(base, "perpetual",
                rewrite(LICENCE, "The licence runs for the term paid for",
                        "The licence is perpetual and runs for the term paid for")),
            "perpetual",
        )

        # 5. a key offered in prose. The binary guard cannot see a document.
        red(
            "a licence key promised in a document",
            run(base, "key",
                rewrite(DOCS, "## How to buy", "## How to buy\n\nYou receive a licence key by e-mail.")),
            "licence key",
        )

        # 6. the guard's own floor: an offer file that parses to nothing agrees
        #    with every document, and must say so instead.
        red(
            "an offer file the parser cannot read",
            run(base, "unparseable", rewrite(OFFER, "[[button]]", "[[knop]]", 4)),
            "parsed to 0",
        )

        # 7. and the same floor from the other side: a document emptied out.
        red(
            "a document emptied",
            run(base, "empty", lambda root: (root / DOCS).write_text("", encoding="utf-8")),
            "empty",
        )

        # 8. a denial is not an offer. This is the case that makes the guard
        #    usable at all: these documents exist partly to say there is no key.
        allowed = run(base, "denial", None)
        check(
            "a sentence denying a licence key stays green",
            allowed.returncode == 0 and "no licence key" in (REPO / DOCS).read_text(encoding="utf-8"),
            allowed.stderr[-300:],
        )

        # 9. restored
        again = run(base, "restored", None)
        check("restored -> green", again.returncode == 0, again.stderr[-300:])

    check("at least 8 cases ran", len(results) >= 8)

    failed = [r for r in results if not r[0]]
    for ok, what, detail in results:
        print(f"  {'ok  ' if ok else 'FAIL'} {what}" + (f" -> {detail}" if not ok and detail else ""))
    if failed:
        print(f"[test_one_offer] {len(failed)} case(s) failed", file=sys.stderr)
        return 1
    print(f"[test_one_offer] {len(results)} case(s); the offer cannot drift between documents quietly")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
