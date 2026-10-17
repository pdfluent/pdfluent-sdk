#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""`no_retired_price_in_a_published_readme.py` still goes red where it must.

A guard over prose is the easiest kind to break by accident: tighten a regex,
rewrap a paragraph, and it matches nothing while still printing a green line.
That is not hypothetical -- it is how the retired prices survived four cleanups
of the website, and how eleven registry READMEs kept selling an evaluation
licence for three days after the model changed (#479).

So each case below copies the real tree into a scratch repository, plants one
regression that actually happened, and requires a non-zero exit that names it.
The unmutated copy is checked first: without that, a guard that fails on
everything would pass this file.

Usage:
    python3 scripts/ci/test_no_retired_price_in_a_published_readme.py
"""
from __future__ import annotations

import pathlib
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = "scripts/ci/no_retired_price_in_a_published_readme.py"

# What the scratch tree has to carry for the guard to reach its floors: every
# path it reads, plus the manifests it derives the priced-crate list from.
TREES = ("crates", "bindings", "docs/en", "docs/cookbook", "docs/hero",
         "packaging/chocolatey", "pdfluent-examples")

# The CLI's own help text, which is a published surface too. The clap
# definition lives in the library since the CLI became one (#636).
CLI_MAIN = "crates/pdfluent-cli/src/lib.rs"
FILES = (GUARD, "README.md")

# Crate README carrying the canonical licence block, used by several cases.
CRATE_README = "crates/pdf-annot/README.md"

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
    for rel in TREES:
        shutil.copytree(REPO / rel, root / rel,
                        ignore=shutil.ignore_patterns("target", "assets", "*.pdf",
                                                      "*.png", "node_modules"))
    return root


def rewrite(rel: str, old: str, new: str, count: int = 1):
    def mutate(root: pathlib.Path) -> None:
        path = root / rel
        text = path.read_text(encoding="utf-8")
        assert text.count(old) == count, \
            f"{rel}: {old!r} occurs {text.count(old)}x, not {count}x"
        path.write_text(text.replace(old, new), encoding="utf-8")

    return mutate


def drop(rel: str):
    def mutate(root: pathlib.Path) -> None:
        (root / rel).unlink()

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
        check(f"{what} -> names {token!r}", token in result.stderr,
              result.stderr[-400:])


def main() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        base = pathlib.Path(tmp)

        # 0. the untouched tree passes, or every case below proves nothing.
        clean = run(base, "clean", None)
        check("the unmutated tree passes", clean.returncode == 0,
              clean.stderr[-500:])

        # 1. the price the inventory found quoted back at us by two AI
        #    assistants, planted on the page crates.io shows.
        red(
            "a retired price on a registry README",
            run(base, "price", rewrite(
                CRATE_README,
                "in four options: Commercial (per",
                "from €699, in four options: Commercial (per")),
            "699", "retired on 06-09-2026",
        )

        # 2. the same price in the README hero image, which is HTML and uses
        #    the entity rather than the character.
        red(
            "a retired price in the hero document",
            run(base, "hero", rewrite(
                "docs/hero/hero_document.html",
                '<td class="n">&euro; 999.00</td>\n        <td class="n">&euro; 999.00</td>',
                '<td class="n">&euro; 699.00</td>\n        <td class="n">&euro; 699.00</td>')),
            "699",
        )

        # 3. the wording eleven registry READMEs carried until this issue.
        red(
            "an evaluation licence offered again",
            run(base, "evaluation", rewrite(
                CRATE_README,
                "**AGPL-3.0 or a commercial licence**, at your option",
                "**Free for evaluation. Production use requires a valid license.**")),
            "Free for evaluation",
        )

        # 4. seats. The editor was per-seat until 06-09-2026 and the SDK never
        #    was, which is exactly why the word came back on its own.
        red(
            "a per-seat licence",
            run(base, "seats", rewrite(
                CRATE_README,
                "in four options: Commercial (per\norganisation)",
                "in four options: Commercial (per seat)")),
            "per seat",
        )

        # 5. an activation call for an API that no longer exists -- the .NET
        #    README documented one for three days after the code went.
        red(
            "an activation call in a binding README",
            run(base, "activate", rewrite(
                "bindings/dotnet/README.md",
                "## Licence",
                "## Licence\n\nActivate with `Licensing.ActivateKey(\"tier:enterprise\")`.")),
            "ActivateKey",
        )

        # 6. the trial page. It still resolves, so a stale link does not 404 --
        #    it quietly sells a model we withdrew.
        red(
            "a link to the trial page",
            run(base, "trial", rewrite(
                CRATE_README,
                "- Pricing: <https://pdfluent.com/sdk/pricing>",
                "- Trial: <https://pdfluent.com/trial>")),
            "pdfluent.com/trial",
        )

        # 7. the floor that stops a cleanup by deletion: dropping the licence
        #    section passes a vocabulary scan and tells a buyer nothing.
        red(
            "a priced crate whose README stops naming the price",
            run(base, "no-url", rewrite(
                CRATE_README, "pdfluent.com/sdk/pricing", "pdfluent.com", 3)),
            "does not name pdfluent.com/sdk/pricing",
        )

        # 8. what `--help` prints. Every README beside it had said
        #    AGPL-or-commercial for three days while this still offered an
        #    evaluation, because nothing read the binary's own text.
        red(
            "the CLI offering evaluation use in --help",
            run(base, "cli", rewrite(
                CLI_MAIN,
                "AGPL-3.0 or a commercial licence, at your option.\",",
                "Commercial license; evaluation use permitted.\",")),
            "evaluation use",
        )

        # 9. a quote to ask for. B28 is four buttons: no seats, no keys, no
        #    quotes -- and the Maven licence block asked for one until #479.
        red(
            "a price you have to ask for",
            run(base, "quote", rewrite(
                "bindings/java/pom.xml",
                "the prices are\n                published",
                "contact sales@pdfluent.com for pricing; the prices are\n                published")),
            "no quote",
        )

        # 10. the floor that stops a rename from silently emptying the scan.
        red(
            "a published surface that moved out of the list",
            run(base, "moved", drop("docs/hero/hero_document.html")),
            "docs/hero/hero_document.html",
        )

    failed = [r for r in results if not r[0]]
    for ok, what, detail in results:
        print(f"{'ok  ' if ok else 'FAIL'} {what}")
        if not ok and detail:
            print(f"       {detail}")
    print(f"\n{len(results) - len(failed)}/{len(results)} checks passed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
