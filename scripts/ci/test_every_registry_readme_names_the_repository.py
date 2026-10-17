#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The registry-README guard refuses what it should, and only that (#205).

Each case builds a scratch tree that fails in one specific way -- the link
missing, the link present but below the fold, the manifest naming another
repository, the register emptied -- and proves the guard says so. The last block
runs it against the real tree, so dropping the line out of any shipped README
turns this red rather than leaving a guard that describes files nobody has.
"""
from __future__ import annotations

import json
import pathlib
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import every_registry_readme_names_the_repository as guard  # noqa: E402

REPO = guard.REPO
MINIMUM_CASES = 10  # FLOOR


def build(root: pathlib.Path, readme: str, manifest_repo: str | None = REPO) -> dict:
    """A one-channel register over a scratch tree."""
    (root / "pkg").mkdir(parents=True, exist_ok=True)
    (root / "pkg" / "README.md").write_text(readme, encoding="utf-8")
    (root / "pkg" / "Cargo.toml").write_text(
        f'[package]\nname = "x"\nrepository = "{manifest_repo or "https://example.invalid/x"}"\n',
        encoding="utf-8")
    return {"scratch": ("pkg/README.md", "pkg/Cargo.toml")}


GOOD = f"# pdfluent\n\n[Source on GitHub]({REPO}) · [Docs](https://pdfluent.com/docs)\n\nBody.\n"


def main() -> int:
    failures: list[str] = []
    cases = 0

    def case(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:300]}"))
        if not ok:
            failures.append(what)

    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)

        root = tmp / "good"
        reg = build(root, GOOD)
        case("a README naming the repository above the fold passes",
             guard.judge(root, reg) == [], str(guard.judge(root, reg)))

        root = tmp / "absent"
        reg = build(root, "# pdfluent\n\nBody with no link at all.\n")
        p = guard.judge(root, reg)
        case("a README that never names the repository is fatal",
             any("does not name" in x for x in p), str(p))

        # The failure this guard is really for: the link exists, and nobody
        # meeting the page ever gets to it.
        root = tmp / "buried"
        reg = build(root, "# pdfluent\n\n" + "Filler paragraph.\n\n" * 8 +
                    f"[Source]({REPO})\n")
        p = guard.judge(root, reg)
        case("a link below the fold is fatal, not a pass",
             any("first 6 lines" in x for x in p), str(p))

        # The boundary, from both sides: the sixth non-blank line is inside the
        # fold and the seventh is not. A guard whose edge nobody pinned is one
        # whose edge moves the next time somebody re-wraps a paragraph.
        root = tmp / "edge-in"
        reg = build(root, "# pdfluent\n\na\n\nb\n\nc\n\nd\n\n" + f"[Source]({REPO})\n")
        p = guard.judge(root, reg)
        case("the sixth non-blank line is still above the fold", p == [], str(p))

        root = tmp / "edge-out"
        reg = build(root, "# pdfluent\n\na\n\nb\n\nc\n\nd\n\ne\n\n" + f"[Source]({REPO})\n")
        p = guard.judge(root, reg)
        case("the seventh is not",
             any("first 6 lines" in x for x in p), str(p))

        root = tmp / "manifest"
        reg = build(root, GOOD, manifest_repo=None)
        p = guard.judge(root, reg)
        case("a manifest pointing somewhere else is fatal",
             any("its README" in x for x in p), str(p))

        root = tmp / "missing-readme"
        reg = build(root, GOOD)
        (root / "pkg" / "README.md").unlink()
        p = guard.judge(root, reg)
        case("a missing README is fatal", any("is missing" in x for x in p), str(p))

        # The floor is about the SHIPPED register, so it is exercised by
        # emptying that one rather than by passing an empty fixture -- a fixture
        # with one channel is a test, not a register somebody hollowed out.
        held = guard.CHANNELS
        try:
            guard.CHANNELS = {"crates.io": held["crates.io"]}
            p = guard.judge()
            case("an emptied shipped register is fatal rather than vacuously green",
                 any("floor" in x for x in p), str(p))
        finally:
            guard.CHANNELS = held

        # npm's manifest is JSON and spells the url as `git+https://…`, which a
        # plain substring over the file would still find -- but the guard parses
        # it, so a repository field that is an object is read as one.
        root = tmp / "npm"
        (root / "pkg").mkdir(parents=True)
        (root / "pkg" / "README.md").write_text(GOOD, encoding="utf-8")
        (root / "pkg" / "package.json").write_text(json.dumps(
            {"name": "@pdfluent/node",
             "repository": {"type": "git", "url": f"git+{REPO}.git"}}), encoding="utf-8")
        p = guard.judge(root, {"npm": ("pkg/README.md", "pkg/package.json")})
        case("a JSON manifest spelling the url as git+… still counts",
             p == [], str(p))

    # --- and the tree this actually ships -------------------------------------
    p = guard.judge()
    case("every shipped registry README and manifest names the repository",
         p == [], str(p))

    case("the case floor is met", cases >= MINIMUM_CASES,
         f"{cases} cases, floor {MINIMUM_CASES}")

    print(f"\n[registry-readme-test] {cases - len(failures)}/{cases} passed")
    if failures:
        for f in failures:
            print(f"  FAILED: {f}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
