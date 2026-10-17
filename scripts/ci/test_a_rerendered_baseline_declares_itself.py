#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Cases for a_rerendered_baseline_declares_itself.py (#293).

A guard that finds nothing on a clean branch cannot tell you whether it still
recognises anything -- the shape #293 opened with, where `avrt.yml` was green on
42 runs having compared zero images. So every case here builds a scratch
repository that actually moves a baseline page, and checks the guard says so.

The last block is the one that keeps the guard honest about this repository
rather than about a fixture: it asserts the real tree's own range is judged
without crashing, and that the guard is wired into the local gate. Deleting the
gate line turns this red.
"""
from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
GUARD = HERE / "a_rerendered_baseline_declares_itself.py"
REPO = HERE.parents[1]
BASELINE = "crates/visual-regression/baseline"
MINIMUM_CASES = 15  # FLOOR: a suite that has quietly emptied proves nothing.

sys.path.insert(0, str(HERE))
from fixture_env import sealed_env  # noqa: E402


def clean_env(cwd: pathlib.Path | None = None) -> dict[str, str]:
    """Without the caller's GIT_*, and with the config surface sealed.

    A hook hands GIT_DIR and GIT_WORK_TREE down as absolute paths; git then
    ignores the directory it was pointed at, and a test that builds scratch
    repositories starts editing the real one. On 25-08-2026 that set
    `core.bare = true` here and stopped thirty worktrees.
    """
    return sealed_env(cwd=cwd, identity=True)


def git(wd: pathlib.Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["git", *args], cwd=str(wd), capture_output=True,
                          text=True, env=clean_env(cwd=wd))


def guard(wd: pathlib.Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(GUARD), *args], cwd=str(wd),
                          capture_output=True, text=True, env=clean_env())


def repo(tmp: pathlib.Path) -> pathlib.Path:
    """A scratch repository with two baseline pages and a renderer file."""
    wd = tmp / "r"
    (wd / BASELINE).mkdir(parents=True)
    git(wd, "init", "-q", "-b", "master")
    git(wd, "config", "user.email", "t@t")
    git(wd, "config", "user.name", "t")
    (wd / BASELINE / "shading-axial-p1.png").write_bytes(b"\x89PNG-one")
    (wd / BASELINE / "text-modes-p1.png").write_bytes(b"\x89PNG-two")
    (wd / "crates" / "pdf-render").mkdir(parents=True)
    (wd / "crates" / "pdf-render" / "lib.rs").write_text("fn draw() {}\n")
    git(wd, "add", "-A")
    git(wd, "commit", "-qm", "base")
    git(wd, "checkout", "-q", "-b", "topic")
    return wd


def rerender(wd: pathlib.Path, page: str, content: bytes) -> None:
    (wd / BASELINE / page).write_bytes(content)


def main() -> int:
    failures: list[str] = []
    cases = 0

    def expect(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:300]}"))
        if not ok:
            failures.append(what)

    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)

        # --- the thing it exists for -----------------------------------------
        wd = repo(tmp / "a")
        rerender(wd, "shading-axial-p1.png", b"\x89PNG-moved")
        (wd / "crates" / "pdf-render" / "lib.rs").write_text("fn draw() { aa(); }\n")
        git(wd, "commit", "-qam", "improve antialiasing")
        r = guard(wd, "--base", "master")
        expect("a renderer change that moves a baseline with it fails",
               r.returncode == 1, f"exit {r.returncode}")
        expect("and it names the page", "shading-axial-p1.png" in r.stderr,
               r.stderr[:200])
        expect("and it says which commit must declare it",
               "Rerenders-deliberately" in r.stderr, r.stderr[:200])

        # --- the declaration that makes it pass -------------------------------
        wd = repo(tmp / "b")
        rerender(wd, "shading-axial-p1.png", b"\x89PNG-moved")
        git(wd, "commit", "-qam",
            "widen the dash phase\n\nRerenders-deliberately: shading-axial-p1.png\n")
        r = guard(wd, "--base", "master")
        expect("a declared re-render passes", r.returncode == 0,
               r.stdout + r.stderr)
        expect("and it says how many pages moved", "1 baseline page" in r.stdout,
               r.stdout[:200])

        # --- one trailer, several pages ---------------------------------------
        wd = repo(tmp / "c")
        rerender(wd, "shading-axial-p1.png", b"\x89PNG-moved")
        rerender(wd, "text-modes-p1.png", b"\x89PNG-moved-too")
        git(wd, "commit", "-qam",
            "new rasteriser\n\nRerenders-deliberately: shading-axial-p1.png, "
            "text-modes-p1.png\n")
        r = guard(wd, "--base", "master")
        expect("a comma-separated trailer covers several pages",
               r.returncode == 0, r.stdout + r.stderr)

        # --- declaring one of two ---------------------------------------------
        wd = repo(tmp / "d")
        rerender(wd, "shading-axial-p1.png", b"\x89PNG-moved")
        rerender(wd, "text-modes-p1.png", b"\x89PNG-moved-too")
        git(wd, "commit", "-qam",
            "new rasteriser\n\nRerenders-deliberately: shading-axial-p1.png\n")
        r = guard(wd, "--base", "master")
        expect("declaring one page of two still fails on the other",
               r.returncode == 1 and "text-modes-p1.png" in r.stderr,
               f"exit {r.returncode}: {r.stderr[:200]}")

        # --- the other direction ----------------------------------------------
        wd = repo(tmp / "e")
        (wd / "crates" / "pdf-render" / "lib.rs").write_text("fn draw() { fix(); }\n")
        git(wd, "commit", "-qam",
            "fix a cap\n\nRerenders-deliberately: text-modes-p1.png\n")
        r = guard(wd, "--base", "master")
        expect("a trailer naming a page nothing moves fails",
               r.returncode == 1 and "does not change" in r.stderr,
               f"exit {r.returncode}: {r.stderr[:200]}")

        # --- a full path in the trailer means the same page --------------------
        wd = repo(tmp / "f")
        rerender(wd, "shading-axial-p1.png", b"\x89PNG-moved")
        git(wd, "commit", "-qam",
            "resample\n\nRerenders-deliberately: "
            f"{BASELINE}/shading-axial-p1.png\n")
        r = guard(wd, "--base", "master")
        expect("a trailer spelling the whole path is the same declaration",
               r.returncode == 0, r.stdout + r.stderr)

        # --- a declaration in the wrong commit --------------------------------
        # The commit that must carry it is the LAST one to move the page: an
        # earlier commit's declaration can belong to a change a later one undid.
        wd = repo(tmp / "g")
        rerender(wd, "shading-axial-p1.png", b"\x89PNG-first")
        git(wd, "commit", "-qam",
            "first pass\n\nRerenders-deliberately: shading-axial-p1.png\n")
        rerender(wd, "shading-axial-p1.png", b"\x89PNG-second")
        git(wd, "commit", "-qam", "second pass")
        r = guard(wd, "--base", "master")
        expect("a declaration in an earlier commit does not cover a later move",
               r.returncode == 1, f"exit {r.returncode}: {r.stderr[:200]}")

        # --- an added page and a removed page are both moves -------------------
        wd = repo(tmp / "h")
        rerender(wd, "rotated-270-p1.png", b"\x89PNG-new")
        git(wd, "add", "-A")
        git(wd, "commit", "-qm", "add a fixture")
        r = guard(wd, "--base", "master")
        expect("a new baseline page must be declared too",
               r.returncode == 1 and "rotated-270-p1.png" in r.stderr,
               f"exit {r.returncode}: {r.stderr[:200]}")

        wd = repo(tmp / "i")
        git(wd, "rm", "-q", f"{BASELINE}/text-modes-p1.png")
        git(wd, "commit", "-qm", "drop a fixture")
        r = guard(wd, "--base", "master")
        expect("a removed baseline page must be declared too",
               r.returncode == 1 and "text-modes-p1.png" in r.stderr,
               f"exit {r.returncode}: {r.stderr[:200]}")

        # --- prose about a trailer is not a trailer ----------------------------
        # This guard's own first commit message quoted the format as an indented
        # example, and the first version read that example as a declaration and
        # refused the commit that introduced it.
        wd = repo(tmp / "p")
        (wd / "crates" / "pdf-render" / "lib.rs").write_text("fn draw() { doc(); }\n")
        git(wd, "commit", "-qam",
            "document the format\n\nWrite it like this:\n\n"
            "    Rerenders-deliberately: shading-axial-p1.png\n")
        r = guard(wd, "--base", "master")
        expect("an indented example is not a declaration",
               r.returncode == 0, r.stdout + r.stderr)

        # --- a range that moves nothing ---------------------------------------
        wd = repo(tmp / "j")
        (wd / "crates" / "pdf-render" / "lib.rs").write_text("fn draw() { ok(); }\n")
        git(wd, "commit", "-qam", "an ordinary renderer change")
        r = guard(wd, "--base", "master")
        expect("a range that moves no baseline passes and says so",
               r.returncode == 0 and "no baseline page" in r.stdout,
               r.stdout + r.stderr)

        # --- a comparison that did not happen is not a pass ---------------------
        wd = repo(tmp / "k")
        r = guard(wd, "--base", "topic")
        expect("an empty range refuses rather than reporting clean",
               r.returncode == 2, f"exit {r.returncode}: {r.stderr[:200]}")

        r = guard(wd, "--base", "no-such-ref")
        expect("an unresolvable base refuses rather than reporting clean",
               r.returncode == 2, f"exit {r.returncode}: {r.stderr[:200]}")

    # --- and the repository this actually ships -------------------------------
    gate = (REPO / "scripts" / "ci" / "local_ci_gate.sh").read_text(encoding="utf-8")
    expect("the guard is wired into the local gate",
           "a_rerendered_baseline_declares_itself.py" in gate,
           "not found in local_ci_gate.sh")
    expect("the baseline directory it guards is really there",
           (REPO / BASELINE).is_dir() and
           any((REPO / BASELINE).glob("*.png")),
           f"{BASELINE} holds no PNG")

    expect("the case floor is met", cases >= MINIMUM_CASES,
           f"{cases} cases, floor {MINIMUM_CASES}")

    print(f"\n[baseline-test] {cases - len(failures)}/{cases} passed")
    if failures:
        for f in failures:
            print(f"  FAILED: {f}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
