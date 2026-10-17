#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""The landing-page guard bites on a page that lost a row, and only then.

A guard over a README finds nothing on almost every run, so green means nothing
until it has been shown to go red -- and shown to stay green on a page that was
merely rewritten. Both halves are here: one case per row it is supposed to
notice, and one that rewrites every sentence and keeps the rows.

The guard reads `REPO / "README.md"` and `REPO / "docs/readme-hero.png"`, so
each case vendors both into a scratch directory at the same depth and hands them
content of the case's own making. The hero cases pass `hero=` to plant an absent
file, a placeholder that is not a PNG, or one over the byte budget.
"""

from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
GUARD = HERE / "the_public_readme_is_a_landing_page.py"
REAL_README = HERE.parent.parent / "README.md"

MINIMUM_CASES = 16  # FLOOR: fewer means a case went missing, not that all is well

HERO = "docs/readme-hero.png"
PNG_MAGIC = b"\x89PNG\r\n\x1a\n"
# A PNG header and nothing else. The guard reads the first eight bytes and the
# length, so a real image would only make the test slower and the failures
# harder to read.
STUB_PNG = PNG_MAGIC + b"\x00" * 64


def run(root: pathlib.Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(root / "scripts" / "ci" / GUARD.name)],
                          capture_output=True, text=True, timeout=60)


def scratch(tmp: pathlib.Path, readme: str, hero: bytes | None = STUB_PNG) -> pathlib.Path:
    root = tmp / "r"
    (root / "scripts" / "ci").mkdir(parents=True)
    (root / "scripts" / "ci" / GUARD.name).write_text(GUARD.read_text(encoding="utf-8"),
                                                      encoding="utf-8")
    (root / "README.md").write_text(readme, encoding="utf-8")
    if hero is not None:
        (root / HERO).parent.mkdir(parents=True, exist_ok=True)
        (root / HERO).write_bytes(hero)
    return root


def main() -> int:
    if not GUARD.is_file():
        print(f"SKIPPED (not a pass): {GUARD} is missing", file=sys.stderr)
        return 3
    if not REAL_README.is_file():
        print(f"SKIPPED (not a pass): {REAL_README} is missing", file=sys.stderr)
        return 3

    real = REAL_README.read_text(encoding="utf-8")
    failures: list[str] = []
    cases = 0

    def expect(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}")
        if not ok:
            failures.append(f"{what}: {detail}")

    def case(readme: str, hero: bytes | None = STUB_PNG) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as d:
            return run(scratch(pathlib.Path(d), readme, hero))

    # --- the control ----------------------------------------------------
    # This repository's own README. If it fails here, every red case below is
    # red for a reason the case did not plant.
    real_hero = (REAL_README.parent / HERO).read_bytes() if (REAL_README.parent / HERO).is_file() else None
    r = case(real, real_hero)
    expect("the README this repository ships passes, with the hero it ships",
           r.returncode == 0, (r.stdout + r.stderr)[-400:])

    # --- one case per row it must notice --------------------------------
    # Each removes exactly one thing from the real page and expects exactly that
    # to be named. A rewrite that drops a registry is the realistic shape here:
    # nobody deletes the badge row, somebody reorganises it.
    for removed, what in (
        ("crates.io/crates/pdfluent", "the crates.io badge"),
        ("pypi.org/project/pdfluent", "the PyPI badge"),
        ("npmjs.com/package/@pdfluent/node", "the npm badge"),
        ("nuget.org/packages/PDFluent", "the NuGet badge"),
        ("central.sonatype.com/artifact/com.pdfluent/pdfluent", "the Maven badge"),
        ("cargo add pdfluent", "the Rust install line"),
        ("dotnet add package PDFluent", "the .NET install line"),
        ("<artifactId>pdfluent</artifactId>", "the Java dependency"),
    ):
        r = case(real.replace(removed, "REMOVED-BY-THE-TEST"))
        expect(f"a page that lost {what} fails",
               r.returncode == 1 and "are missing" in r.stderr,
               (r.stdout + r.stderr)[-300:])

    # The note that accounts for the third-party hosts serving images here. A
    # host in our README that nobody wrote down is what #205 asked us to avoid,
    # and that half of the rule outlives the badge it was written for.
    r = case(real.replace("External assets in this README", "Notes"))
    expect("a page whose external assets are not accounted for fails",
           r.returncode == 1 and "accounts for its external assets" in r.stderr,
           (r.stdout + r.stderr)[-300:])

    # The star badge came back. It showed a zero -- a badge advertising how few
    # stars a project has argues against the project -- and it was removed on
    # 07-09-2026. Both spellings, because the same widget under a new hostname
    # is the same widget.
    for host, url in (("the worker", "star-prompt-worker.lnmput.workers.dev/badge?repo=x"),
                      ("the service itself", "starme.dev/badge/pdfluent")):
        r = case(real.replace("## Licence", f"[![Star](https://{url})](x)\n\n## Licence"))
        expect(f"a page that puts the star badge back fails ({host})",
               r.returncode == 1 and "no star badge" in r.stderr,
               (r.stdout + r.stderr)[-300:])

    # --- the hero, and the three ways it stops being real ----------------
    # A reference that no longer points anywhere, a placeholder dropped in its
    # place, and an image that grew past what the first screen can afford. All
    # three leave a README that still reads as though it shows real output.
    r = case(real.replace(HERO, "docs/somewhere-else.png"))
    expect("a page that stopped naming the hero on the first screen fails",
           r.returncode == 1 and "real output on the first screen" in r.stderr,
           (r.stdout + r.stderr)[-300:])

    r = case(real, hero=None)
    expect("a hero the README points at but the tree does not have fails",
           r.returncode == 1 and "the tree does not have it" in r.stderr,
           (r.stdout + r.stderr)[-300:])

    r = case(real, hero=b"a placeholder, not an image")
    expect("a hero that is not a PNG fails",
           r.returncode == 1 and "is not a PNG" in r.stderr,
           (r.stdout + r.stderr)[-300:])

    r = case(real, hero=PNG_MAGIC + b"\x00" * (400 * 1024))
    expect("a hero over the byte budget fails",
           r.returncode == 1 and "over the" in r.stderr and "ceiling" in r.stderr,
           (r.stdout + r.stderr)[-300:])

    # --- the badge row has to be on the first screen ---------------------
    # Not "somewhere in the file": #205 asked for the first screen. A row pushed
    # below the features satisfies every other check here and is seen by nobody.
    head, _, rest = real.partition("## Install")
    moved = "# PDFluent\n\n" + ("filler\n" * 40) + head + "## Install" + rest
    r = case(moved)
    expect("a badge row pushed below the first screen fails",
           r.returncode == 1 and "first screen" in r.stderr,
           (r.stdout + r.stderr)[-300:])

    # --- and the floor ---------------------------------------------------
    # A guard reading a stub finds nothing missing in it and reports the same
    # green as one reading the real page.
    r = case("# PDFluent\n\nA PDF SDK.\n")
    expect("a stub README is FATAL, not a pass",
           r.returncode == 1 and "below the floor" in r.stderr,
           (r.stdout + r.stderr)[-300:])

    # --- what it must NOT do ---------------------------------------------
    # Every sentence rewritten, every row kept. A guard that fails here is a
    # guard that will be switched off the first time somebody improves the prose.
    rewritten = "\n".join(
        line if (line.strip().startswith(("[!", "cargo ", "pip ", "npm ", "dotnet ",
                                            "<", "["))
                  or "External assets in this README" in line
                  or HERO in line)
        else ("Rewritten prose." if line.strip() else "")
        for line in real.splitlines())
    r = case(rewritten)
    expect("a page whose prose was entirely rewritten still passes",
           r.returncode == 0, (r.stdout + r.stderr)[-400:])

    if cases < MINIMUM_CASES:
        print(f"FLOOR: {cases} case(s) ran, fewer than {MINIMUM_CASES}. A case went "
              "missing, which is not the same as all being well.", file=sys.stderr)
        return 1

    if failures:
        print(f"{len(failures)} of {cases} case(s) failed:", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 1

    print(f"[test-readme] OK: {cases} case(s) -- one per row the guard must notice, "
          "a hero that is missing, fake or over budget, a badge row moved off the "
          "first screen, a stub, and a full rewrite that keeps every row.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
