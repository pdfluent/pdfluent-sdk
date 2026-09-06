#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""The cookbook register is read in every direction that can rot on its own.

`cookbook_recipes.py` decides three things nobody else does: that a recipe names
code which compiles, that a compiled block is either in the cookbook or excused
by name, and that the two cross-links of #167 still point somewhere. Each of
those is the kind of check that can silently drop out -- the register keeps
looking full, the page keeps rendering, and what it renders is a recipe with no
code or a link into a path the seeding strips.

So the judgement lives in `judge()`, apart from the export, and this file
exercises it on synthetic input: milliseconds, on every pull request, rather
than a build. It then turns the same function on the checked-in register, so a
row that went stale costs one round to find.

Mutation-tested, which is the only reason to believe it. Each check in
`cookbook_recipes.py` was removed one at a time and this file went red each
time; the case names below say which removal each one catches.
"""

import importlib.util
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
GUARD = ROOT / "scripts" / "ci" / "cookbook_recipes.py"


def load():
    spec = importlib.util.spec_from_file_location("cookbook_recipes", GUARD)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


BLOCKS = {"alpha": "use pdfluent::PdfDocument;", "beta": "let n = doc.page_count();"}
REASON = "the download page shows this shorter form of the recipe"


def recipe(**over) -> dict:
    base = {
        "slug": "open-a-document",
        "category": "reading",
        "title": "Open a document",
        "intent": "Open a document and print how many pages it carries.",
        "block": "alpha",
    }
    base.update(over)
    return base


_DEFAULT = object()


def register(recipes, excused=_DEFAULT, **source_over) -> dict:
    source = {
        "path": "crates/pdfluent/examples/site_snippets.rs",
        "public_repo": "https://github.com/pdfluent/pdfluent-sdk",
        "public_branch": "main",
        "site_url": "https://pdfluent.com/cookbook/",
    }
    source.update(source_over)
    # `excused or {...}` would be the bug this file exists to catch: an empty
    # register is falsy, so the case that passes one would silently get the
    # default and test nothing.
    if excused is _DEFAULT:
        excused = {"beta": REASON}
    return {"source": source, "recipe": recipes, "not_a_recipe": excused}


README_OK = "the cookbook lives at https://pdfluent.com/cookbook/ and is generated"
NO_INTERNAL = ("corpus/",)


def said(cb, reg, blocks=None, readme=README_OK, internal=NO_INTERNAL):
    return cb.judge(reg, blocks if blocks is not None else BLOCKS, readme, internal)


def cases(cb) -> list[tuple[str, bool, str]]:
    out = []
    # The floor is 8 and every synthetic register here is smaller, so each case
    # would report that too. Dropping it keeps every case about one thing.
    def without_floor(lines):
        return [x for x in lines if not x.startswith("FLOOR")]

    ok = without_floor(said(cb, register([recipe()])))
    out.append(("a well-formed register raises nothing", ok == [], f"said={ok}"))

    # 1. A recipe whose block is not in the example file. The page renders a
    #    heading, an intent, and no code -- and looks fine in a diff.
    got = without_floor(said(cb, register([recipe(block="ghost")], excused={"alpha": REASON, "beta": REASON})))
    out.append((
        "a recipe naming a block that does not exist is an error",
        len(got) == 1 and "no `// site:` marker" in got[0],
        f"said={got}",
    ))

    # 2. THE DIRECTION THAT GOES MISSING: a compiled block that is neither a
    #    recipe nor excused. Nothing fails, the block simply never reaches the
    #    cookbook, and the next example added does the same.
    got = without_floor(said(cb, register([recipe()], excused={})))
    out.append((
        "a block that is neither a recipe nor excused is an error",
        len(got) == 1 and "neither carries it nor says why not" in got[0],
        f"said={got}",
    ))

    # 3. An excuse for a block that IS a recipe says the opposite of the truth.
    got = without_floor(said(cb, register([recipe()], excused={"alpha": REASON, "beta": REASON})))
    out.append((
        "an excuse for a block that is a recipe is an error",
        any("IS a recipe" in c for c in got),
        f"said={got}",
    ))

    # 4. An excuse for a block the example file no longer carries.
    got = without_floor(said(cb, register([recipe()], excused={"beta": REASON, "gone": REASON})))
    out.append((
        "an excuse for a block that no longer exists is an error",
        any("no `// site:` marker carries this name" in c for c in got),
        f"said={got}",
    ))

    # 5. A reason that is a label. "later" excused blocks for weeks elsewhere.
    got = without_floor(said(cb, register([recipe()], excused={"beta": "later"})))
    out.append((
        "a one-word reason is refused",
        any("A label is not a reason" in c for c in got),
        f"said={got}",
    ))

    # 6. Two recipes on one slug: one of them has no address.
    got = without_floor(said(cb, register([recipe(), recipe(block="beta")], excused={})))
    out.append((
        "two recipes on one slug is an error",
        any("two recipes claim this slug" in c for c in got),
        f"said={got}",
    ))

    # 7. One block twice: the same code under two headings, which is what the
    #    route diet of #277 spent a day undoing.
    got = without_floor(said(cb, register([recipe(), recipe(slug="second-one")], excused={"beta": REASON})))
    out.append((
        "one block under two recipes is an error",
        any("already a recipe" in c for c in got),
        f"said={got}",
    ))

    # 8. A slug that is not a URL segment.
    got = without_floor(said(cb, register([recipe(slug="Open A Document")])))
    out.append((
        "a slug that is not lowercase-hyphenated is refused",
        any("it is a URL" in c for c in got),
        f"said={got}",
    ))

    # 9. A heading nobody decided on.
    got = without_floor(said(cb, register([recipe(category="misc")])))
    out.append((
        "a category outside the closed set is refused",
        any("is not one of" in c for c in got),
        f"said={got}",
    ))

    # 10. Dutch on the English site. The code half of this rule already cost a
    #     round: `let rapport` reached the live page (#247).
    got = without_floor(said(cb, register([recipe(intent="Het aantal bladzijden van een document.")])))
    out.append((
        "a Dutch word in an intent is refused",
        any("Dutch word" in c for c in got),
        f"said={got}",
    ))

    # 11. An intent of two words is a label again, in the other field.
    got = without_floor(said(cb, register([recipe(intent="Opens documents.")])))
    out.append((
        "a two-word intent is refused",
        any("make it a sentence" in c for c in got),
        f"said={got}",
    ))

    # 12. THE ONE THAT ONLY BITES AFTER PUBLICATION: the example file sits in a
    #     path PUBLIC_TREE.toml calls internal. Everything here still works; the
    #     link the site publishes 404s in the public repository, and no guard on
    #     this side sees a published repository.
    got = without_floor(said(cb, register([recipe()]), internal=("crates/pdfluent/examples/",)))
    out.append((
        "a source path the seeding strips is an error",
        any("would 404 in the public" in c for c in got),
        f"said={got}",
    ))

    # 13. A path that is not in the tree at all.
    got = without_floor(said(cb, register([recipe()], path="crates/pdfluent/examples/gone.rs")))
    out.append((
        "a source path that is not a file is an error",
        any("is not a file in this repository" in c for c in got),
        f"said={got}",
    ))

    # 14. THE RETURN HALF. A README that stops linking to the cookbook breaks
    #     the direction #167 is actually about -- the developer who lands on the
    #     repository -- and it breaks it in total silence.
    got = without_floor(said(cb, register([recipe()]), readme="# PDFluent SDK\n"))
    out.append((
        "a README without the link back to the cookbook is an error",
        any("half of the cross-link" in c for c in got),
        f"said={got}",
    ))

    # 15. The floor itself, which every case above suppresses.
    got = said(cb, register([recipe()]))
    out.append((
        "a register below the recipe floor is an error",
        any(c.startswith("FLOOR") for c in got),
        f"said={got}",
    ))

    return out


def the_url(cb) -> list[tuple[str, bool, str]]:
    """The public URL is derived from the path, so the two cannot disagree."""
    out = []
    source = register([recipe()])["source"]
    url = cb.source_url(source)
    out.append((
        "the public URL is the public repository plus the path",
        url == "https://github.com/pdfluent/pdfluent-sdk/blob/main/"
               "crates/pdfluent/examples/site_snippets.rs",
        url,
    ))
    moved = dict(source, path="crates/pdfluent/examples/elsewhere.rs")
    out.append((
        "moving the example file moves the URL with it",
        cb.source_url(moved).endswith("elsewhere.rs"),
        cb.source_url(moved),
    ))
    return out


def the_digest(cb) -> list[tuple[str, bool, str]]:
    """What the website hashes: the rows and the code, and nothing else."""
    out = []
    base = [recipe()]
    d = cb.digest_of(base, BLOCKS)

    out.append((
        "editing the code of a block changes the digest",
        cb.digest_of(base, dict(BLOCKS, alpha="use pdfluent::Nothing;")) != d,
        "unchanged" if cb.digest_of(base, dict(BLOCKS, alpha="x")) == d else "changed",
    ))
    out.append((
        "editing the intent changes the digest",
        cb.digest_of([recipe(intent="Prints the number of pages in a document.")], BLOCKS) != d,
        "compared",
    ))
    out.append((
        "the digest does not depend on the order of the fields",
        cb.digest_of([dict(reversed(list(recipe().items())))], BLOCKS) == d,
        "compared",
    ))
    return out


def the_real_register(cb) -> list[tuple[str, bool, str]]:
    """The checked-in register, judged by the same function."""
    import tomllib

    reg = tomllib.loads((ROOT / "docs" / "site" / "cookbook.toml").read_text(encoding="utf-8"))
    blocks = cb.ontleed()
    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    got = cb.judge(reg, blocks, readme, cb.internal_paths())
    out = [("the checked-in register holds", got == [], f"said={got}")]

    export = ROOT / "docs" / "site" / "cookbook.json"
    payload = json.loads(export.read_text(encoding="utf-8")) if export.is_file() else {}
    out.append((
        "the exported digest is the digest of the register it was written from",
        payload.get("digest") == cb.digest_of(reg["recipe"], blocks),
        f"exported={payload.get('digest')}",
    ))
    out.append((
        "every recipe in the export is a recipe in the register",
        [r["slug"] for r in payload.get("recipes", [])] == [r["slug"] for r in reg["recipe"]],
        f"exported={[r['slug'] for r in payload.get('recipes', [])]}",
    ))
    return out


def main() -> int:
    cb = load()
    results = cases(cb) + the_url(cb) + the_digest(cb) + the_real_register(cb)

    # FLOOR: cases >= 18 -- this file builds its own cases, so a halved list is
    # a gutted file and not a clean tree, and zero cases that all pass is green.
    if len(results) < 18:
        print(
            f"[cookbook-register] only {len(results)} cases. This file builds them "
            "itself, so that is not a clean tree.",
            file=sys.stderr,
        )
        return 1

    for name, ok, _ in results:
        print(f"  {'ok  ' if ok else 'FAIL'}  {name}")

    failed = [(n, t) for n, ok, t in results if not ok]
    if failed:
        print("\n[cookbook-register] the register is no longer read in every direction:\n",
              file=sys.stderr)
        for name, got in failed:
            print(f"  - {name}\n      got: {got}", file=sys.stderr)
        print(
            "\n  The two that go missing first: a compiled block that reaches no "
            "recipe,\n  and the README link back -- neither of them makes anything "
            "look wrong.\n",
            file=sys.stderr,
        )
        return 1

    print(f"[cookbook-register] {len(results)} cases, all good")
    return 0


if __name__ == "__main__":
    sys.exit(main())
