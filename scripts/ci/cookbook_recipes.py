#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""The cookbook shows compiled code, and both cross-links still point somewhere.

#247 put the code on pdfluent.com behind `// site:<name>` markers in
`crates/pdfluent/examples/site_snippets.rs`, so a block that stops compiling
breaks the build instead of a visitor's first impression. #167 asks the next
question: a developer who lands on the repository is told nothing about the
explanation, and a developer who lands on the explanation is told nothing about
the code. Both halves were dead ends.

`docs/site/cookbook.toml` is the seam, and this guard reads it in every
direction that can rot on its own:

  1. a recipe naming a block that is not in the example file          -- the page
     would show nothing, or worse, whatever the site still had cached
  2. a block that is neither a recipe nor excused                     -- a new
     example silently missing from the cookbook
  3. an excuse for a block that no longer exists, or for one that IS a recipe
  4. a source path that `docs/PUBLIC_TREE.toml` calls internal        -- the site
     links a reader straight at that file in the published repository, and the
     seeding would strip it: a 404 nobody finds until a visitor does
  5. a README that no longer carries the link back to the cookbook    -- the
     return half of a cross-link is the half that goes stale in silence
  6. Dutch in a title or an intent, which the English site would show verbatim

What it deliberately does NOT do is resolve a URL. Nothing in this pipeline
reaches the network, and a guard that needs the network is a guard that is
skipped. What can be settled offline is settled here: the path exists, and it
survives publication.

`--write` exports `docs/site/cookbook.json` for the website, with a digest over
the recipes and the code they name; the website refuses to build when its copy
hashes to something else. `--check` fails if that export has drifted from this
register.

Exit codes:
  0  the register, the example file, the manifest and the README agree
  1  any of the above, or the export is out of date
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
REGISTER = REPO / "docs" / "site" / "cookbook.toml"
EXPORT = REPO / "docs" / "site" / "cookbook.json"
MANIFEST = REPO / "docs" / "PUBLIC_TREE.toml"
README = REPO / "README.md"

sys.path.insert(0, str(Path(__file__).resolve().parent))
from extract_site_snippets import NEDERLANDSE_WOORDEN, ontleed  # noqa: E402

# FLOOR: recipes >= 8. A register that has lost most of its rows still passes
# every check above -- each remaining row is fine -- and publishes a cookbook
# with two recipes in it. Same reason `extract_site_snippets.py` refuses to run
# on fewer than three blocks (#235).
MIN_RECIPES = 8

# A reason shorter than this is a label, not a reason. Same rule and the same
# number as the site-blocks exception register, which had entries reading
# "later" before it was enforced (#247).
MIN_REASON_WORDS = 5

SLUG = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
CATEGORIES = ("reading", "pages", "forms", "compliance", "signatures")

DUTCH = re.compile(r"\b(" + "|".join(NEDERLANDSE_WOORDEN) + r")\b", re.IGNORECASE)


def source_url(source: dict) -> str:
    """The public address of the example file, derived and never typed twice.

    Writing the URL out in the register alongside the path is two spellings of
    one fact, and the one that drifts is the one no reader of this file opens.
    """
    return (
        f"{source['public_repo'].rstrip('/')}/blob/"
        f"{source['public_branch']}/{source['path']}"
    )


def digest_of(recipes: list[dict], blocks: dict[str, str]) -> str:
    """A fingerprint over what the website publishes: the rows and their code.

    Only the fields the page renders go in. A comment in this register, or a
    field added later for something else, must not invalidate a website build
    that shows exactly the same page.
    """
    payload = [
        {
            "slug": r["slug"],
            "category": r["category"],
            "title": r["title"],
            "intent": r["intent"],
            "block": r["block"],
            "code": blocks[r["block"]],
        }
        for r in recipes
    ]
    raw = json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(raw.encode("utf-8")).hexdigest()


def internal_paths() -> tuple[str, ...]:
    if not MANIFEST.is_file():
        raise SystemExit(
            f"[cookbook] {MANIFEST.name} is missing, so nothing can say whether the "
            "file the site links to survives publication."
        )

    return tuple(tomllib.loads(MANIFEST.read_text(encoding="utf-8"))["internal"]["paths"])


def export_payload(register: dict, blocks: dict[str, str]) -> dict:
    source = register["source"]
    recipes = register["recipe"]
    return {
        "_": [
            "Generated by scripts/ci/cookbook_recipes.py from docs/site/cookbook.toml.",
            "The website carries a copy as data/cookbook.json and refuses to build",
            "when its copy hashes to something other than `digest` below.",
        ],
        "source": {
            "path": source["path"],
            "url": source_url(source),
            "site_url": source["site_url"],
        },
        "digest": digest_of(recipes, blocks),
        "recipes": [
            {
                "slug": r["slug"],
                "category": r["category"],
                "title": r["title"],
                "intent": r["intent"],
                "block": r["block"],
            }
            for r in register["recipe"]
        ],
    }


def judge(register: dict, blocks: dict[str, str], readme: str,
          internal: tuple[str, ...]) -> list[str]:
    """Every complaint about the register, as lines. Empty means it holds."""
    out: list[str] = []
    source = register.get("source") or {}
    recipes = register.get("recipe") or []
    excused = register.get("not_a_recipe") or {}

    if len(recipes) < MIN_RECIPES:
        out.append(
            f"FLOOR: {len(recipes)} recipe(s), below {MIN_RECIPES}. Every row left "
            "passes every check, and the cookbook publishes anyway -- with almost "
            "nothing in it."
        )

    seen_slugs: set[str] = set()
    seen_blocks: set[str] = set()
    for r in recipes:
        slug = r.get("slug", "")
        where = f"recipe {slug or '<no slug>'}"
        if not SLUG.match(slug):
            out.append(f"{where}: a slug is lowercase words joined by hyphens; it is a URL.")
        if slug in seen_slugs:
            out.append(f"{where}: two recipes claim this slug, so one of them has no address.")
        seen_slugs.add(slug)

        if r.get("category") not in CATEGORIES:
            out.append(
                f"{where}: category {r.get('category')!r} is not one of "
                f"{', '.join(CATEGORIES)}. A new heading is a decision about the page."
            )

        block = r.get("block", "")
        if block not in blocks:
            out.append(
                f"{where}: names block {block!r}, which no `// site:` marker in the "
                "example file carries. The page would show a recipe with no code."
            )
        if block in seen_blocks:
            out.append(f"{where}: block {block!r} is already a recipe, so the code has two addresses.")
        seen_blocks.add(block)

        for field in ("title", "intent"):
            text = r.get(field, "")
            if len(text.split()) < MIN_REASON_WORDS and field == "intent":
                out.append(
                    f"{where}: the intent is {len(text.split())} word(s). It is the one "
                    "line a reader reads before the code; make it a sentence."
                )
            m = DUTCH.search(text)
            if m:
                out.append(
                    f"{where}: {field} carries the Dutch word {m.group(1)!r}. The site "
                    "this feeds is English, and it would show the line verbatim."
                )

    for block, reason in excused.items():
        if block not in blocks:
            out.append(
                f"not_a_recipe[{block}]: no `// site:` marker carries this name any more. "
                "The line excuses nothing and hides the next block that needs excusing."
            )
        if block in seen_blocks:
            out.append(
                f"not_a_recipe[{block}]: this block IS a recipe. An excuse for something "
                "that is in the cookbook says the opposite of what happened."
            )
        if len(str(reason).split()) < MIN_REASON_WORDS:
            out.append(
                f"not_a_recipe[{block}]: {len(str(reason).split())} word(s) of reason. "
                "A label is not a reason; say which page shows it and why it repeats."
            )

    for block in blocks:
        if block not in seen_blocks and block not in excused:
            out.append(
                f"block {block!r} compiles and the site shows it, but the cookbook neither "
                "carries it nor says why not. Add a [[recipe]] or a line in [not_a_recipe]."
            )

    path = source.get("path", "")
    if not (REPO / path).is_file():
        out.append(
            f"source.path {path!r} is not a file in this repository, so every recipe "
            "links a reader at nothing."
        )
    elif path.startswith(internal):
        out.append(
            f"source.path {path!r} is inside a path docs/PUBLIC_TREE.toml calls internal. "
            "The seeding strips it, so the link the site publishes would 404 in the "
            "public repository -- and nothing on this side would ever notice."
        )

    if source.get("site_url", "") not in readme:
        out.append(
            f"README.md no longer links to {source.get('site_url')!r}. That link is the "
            "half of the cross-link that a reader arriving at the repository needs, and "
            "it is the half that disappears in a rewrite without anybody missing it."
        )

    return out


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--write", action="store_true", help="write docs/site/cookbook.json")
    p.add_argument("--check", action="store_true", help="fail if that export has drifted")
    a = p.parse_args()

    if not REGISTER.is_file():
        print(f"[cookbook] {REGISTER} is missing.", file=sys.stderr)
        return 1

    register = tomllib.loads(REGISTER.read_text(encoding="utf-8"))
    blocks = ontleed()
    if not blocks:
        print(
            "[cookbook] the example file yielded no `// site:` blocks, so every recipe "
            "below would be reported as broken for the wrong reason.",
            file=sys.stderr,
        )
        return 1

    complaints = judge(register, blocks, README.read_text(encoding="utf-8"), internal_paths())
    if complaints:
        print(
            "[cookbook] docs/site/cookbook.toml does not hold:\n"
            + "\n".join(f"  - {c}" for c in complaints),
            file=sys.stderr,
        )
        return 1

    payload = export_payload(register, blocks)
    text = json.dumps(payload, indent=2, ensure_ascii=False) + "\n"

    if a.write:
        EXPORT.write_text(text, encoding="utf-8")
        print(f"[cookbook] {len(payload['recipes'])} recipes written to {EXPORT.relative_to(REPO)}")
        return 0

    if a.check:
        if not EXPORT.is_file():
            print(f"[cookbook] {EXPORT.relative_to(REPO)} is missing; run --write.", file=sys.stderr)
            return 1
        if EXPORT.read_text(encoding="utf-8") != text:
            print(
                f"[cookbook] {EXPORT.relative_to(REPO)} has drifted from the register. "
                "Run this script with --write and take the result over; the website "
                "hashes that file and refuses to build against a copy it does not match.",
                file=sys.stderr,
            )
            return 1

    print(f"[cookbook] {len(payload['recipes'])} recipes, {len(blocks)} blocks, both links in place")
    return 0


if __name__ == "__main__":
    sys.exit(main())
