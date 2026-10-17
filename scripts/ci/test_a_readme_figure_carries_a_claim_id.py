#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The README-claims guard refuses what it should, and only that (#358).

Each case is one README that fails in one specific way -- a percentage with no
ID, the withdrawn offer, an internal name -- or one that must NOT fail: a
figure with its ID, a percentage inside a code block, "PDF/A-1b/2b/3b" and
"0/90/180/270" which look like ratios and are not, and the sentence that says
the withdrawn model does not exist. The last block runs the guard over the real
tree, so putting "99.8% pass rate" back into a crate README turns this red
rather than leaving a guard that describes files nobody has.
"""
from __future__ import annotations

import pathlib
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import a_readme_figure_carries_a_claim_id as guard  # noqa: E402

MINIMUM_CASES = 16  # FLOOR

LICENCE = ("## Licence\n\nAGPL-3.0 or a commercial licence, at your option. "
           f"Sold at {guard.PRICING}.\n")
CLEAN = "# pkg\n\nA PDF crate. Body.\n\n" + LICENCE


def crate(root: pathlib.Path, readme: str, licence: str = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial",
          publish: bool = True) -> str:
    d = root / "crates" / "pkg"
    d.mkdir(parents=True, exist_ok=True)
    (d / "README.md").write_text(readme, encoding="utf-8")
    (d / "Cargo.toml").write_text(
        f'[package]\nname = "pkg"\nreadme = "README.md"\nlicense = "{licence}"\n'
        + ("" if publish else "publish = false\n"), encoding="utf-8")
    return "crates/pkg/README.md"


def main() -> int:
    failures: list[str] = []
    cases = 0

    def case(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:300]}"))
        if not ok:
            failures.append(what)

    def one(readme: str, **kw) -> list[str]:
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            rel = crate(root, readme, **kw)
            return guard.judge(root, [rel], descriptions={})

    p = one(CLEAN)
    case("a README with no figure and the dual licence passes", p == [], str(p))

    p = one(CLEAN + "\nConversion has a 99.8% pass rate on the corpus.\n")
    case("a percentage with no claim ID is fatal",
         any("a percentage with no claim ID" in x for x in p), str(p))

    p = one(CLEAN + "\nConversion has a 99.8% pass rate on the corpus (claim A13).\n")
    case("the same percentage with its claim ID passes", p == [], str(p))

    p = one(CLEAN + "\n```text\n99.8% of the time this is code output\n```\n")
    case("a percentage inside a fenced code block is not a claim", p == [], str(p))

    p = one(CLEAN + "\nThe same code scored 286/300 on macOS.\n")
    case("a ratio such as 286/300 with no ID is fatal",
         any("a ratio" in x for x in p), str(p))

    p = one(CLEAN + "\nValidates PDF/A-1b/2b/3b. Rotation is 0/90/180/270 degrees.\n")
    case("PDF/A-1b/2b/3b and 0/90/180/270 are not ratios", p == [], str(p))

    p = one(CLEAN + "\nIt is 2.7× faster than the stateless chain.\n")
    case("a speed factor with no ID is fatal",
         any("a speed factor" in x for x in p), str(p))

    p = one(CLEAN + "\nMeasured on 300 real-world documents, and on the 20K-PDF corpus.\n")
    case("a document count with no ID is fatal",
         any("a document count" in x for x in p), str(p))

    p = one(CLEAN + "\nA FormCalc interpreter with 90+ built-ins.\n")
    case("an N+ count with no ID is fatal", any("an N+ count" in x for x in p), str(p))

    p = one(CLEAN + "\nRequires Java 11+\n- and a JDK.\n")
    case("a version floor such as Java 11+ is not a count", p == [], str(p))

    p = one(CLEAN + "\n**Free for evaluation. Production use requires a valid license.**\n")
    case("the withdrawn offer is fatal, twice",
         sum("offers" in x for x in p) == 2, str(p))

    p = one(CLEAN + "\nThere is no licence key, no activation call and no tier.\n")
    case("saying the withdrawn model does not exist is allowed", p == [], str(p))

    p = one(CLEAN + "\n- Pricing: <https://pdfluent.com/pricing>\n")
    case("the old pricing route is fatal",
         any("the old pricing route" in x for x in p), str(p))

    p = one(CLEAN + "\nThe SDK runs in Trial mode by default; Licensing.ActivateKey unlocks the paid-tier set.\n")
    case("trial mode, activation and tiers are each fatal",
         sum("offers" in x for x in p) == 3, str(p))

    p = one(CLEAN + "\nThis crate is part of the PDFluent commercial Rust PDF SDK.\n")
    case("'commercial Rust PDF SDK' is fatal",
         any("a grade nobody measured" in x for x in p), str(p))

    p = one(CLEAN + "\n### PdfDocMut (Wave 3)\n\nSee #226 for the decision.\n")
    case("an internal programme name and a tracker reference are each fatal",
         sum("names" in x for x in p) == 2, str(p))

    p = one("# pkg\n\nBody. See LICENSE for the license.\n")
    case("a README that talks about the licence without stating the dual offer is fatal",
         any("never states it" in x for x in p) and any("does not link" in x for x in p), str(p))

    p = one("# pkg\n\nA fork. **MIT OR Apache-2.0**, preserved from upstream.\n",
            licence="Apache-2.0 OR MIT")
    case("a permissive fork is not asked to state the dual offer", p == [], str(p))

    with tempfile.TemporaryDirectory() as d:
        root = pathlib.Path(d)
        crate(root, CLEAN + "\n99.8% pass rate.\n", publish=False)
        (root / "README.md").write_text(CLEAN, encoding="utf-8")
        for rel in guard.ALWAYS[1:]:
            (root / rel).parent.mkdir(parents=True, exist_ok=True)
            (root / rel).write_text(CLEAN, encoding="utf-8")
        p = guard.judge(root, descriptions={})
        case("a crate with publish = false is out of scope, and a tree of six READMEs is under the floor",
             not any("99.8%" in x for x in p) and any("floor" in x for x in p)
             and any("no README carries a claim ID" in x for x in p), str(p))

    with tempfile.TemporaryDirectory() as d:
        root = pathlib.Path(d)
        rel = crate(root, CLEAN)
        (root / "pyproject.toml").write_text(
            'description = "Enterprise PDF SDK for Python. 99% accurate."\n', encoding="utf-8")
        p = guard.judge(root, [rel], descriptions={
            "PyPI": ("pyproject.toml", r'^description\s*=\s*"(.*)"')})
        case("a registry description is held to the same offer and figure rules",
             any("PyPI description" in x and "Enterprise PDF SDK" in x for x in p)
             and any("a percentage" in x for x in p), str(p))

    # --- and the tree this actually ships -------------------------------------
    p = guard.judge()
    case("every shipped README and registry description is clean", p == [], "\n".join(p))

    case("the case floor is met", cases >= MINIMUM_CASES,
         f"{cases} cases, floor {MINIMUM_CASES}")

    print(f"\n[readme-claims-test] {cases - len(failures)}/{cases} passed")
    if failures:
        for f in failures:
            print(f"  FAILED: {f}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
