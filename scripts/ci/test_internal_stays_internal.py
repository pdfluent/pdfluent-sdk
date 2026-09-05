#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Does the internal-document guard still bite, in both directions?

`internal_stays_internal.py` has run in no job since it was written (#222). It
was booked in `every_guard_has_a_job.py` as "wired with the other publication
guards after #1543"; #1543 landed on 05-09-2026 and nothing wired it. Measured
the same day: it was red on master, on a source file that named a corpus
document, and that is the one shape nobody would have seen.

Wiring it is half the repair. The other half is this file: a guard nothing runs
and a guard that has stopped recognising anything are indistinguishable from
outside, and the only way to tell them apart is to plant what it must find.

Every case builds a throwaway git repository, vendors the real guard into it
with nothing changed, writes a manifest sized for the fixture, and runs it
there. So a case passes or fails on the guard's logic and never on what this
repository happens to track today.

Exit codes:
  0  every case passed
  1  a case failed, or fewer cases ran than the floor
  3  cannot check (announced, never silent)
"""
from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
GUARD = HERE / "internal_stays_internal.py"
sys.path.insert(0, str(HERE))
from fixture_env import sealed_env  # noqa: E402

MINIMUM_CASES = 26  # FLOOR: fewer means a case went missing, not that all is well

# Two fixture document names, in the shape the real pattern uses: an eight-hex
# prefix and a series name. They are invented; naming a real one here would put
# it in a published file, which is the thing the guard exists to prevent.
NAMES = ["deadbeef_fixture01", "cafed00d_fixture02", "fixture_input_form"]
PATTERN = r"\b(deadbeef_fixture01|cafed00d_fixture02|fixture_input_form)\b"


def git(root: pathlib.Path, *args: str) -> None:
    subprocess.run(["git", *args], cwd=str(root), check=True,
                   capture_output=True, text=True,
                   env=sealed_env(identity=True, cwd=root))


def manifest(*, paths: list[str] | None = None, files: list[str] | None = None,
             pattern: str = PATTERN, floor_paths: int = 2,
             floor_names: int = 3) -> str:
    # The manifest is on its own list, exactly as the real one is: it spells the
    # document names out in the pattern, so a tree that carried it would carry
    # the corpus description the names are kept back for.
    default_paths = ["private/", "corpus/", "docs/PUBLIC_TREE.toml"]
    return (
        "[internal]\n"
        f"paths = {list(paths if paths is not None else default_paths)!r}\n"
        f"files = {list(files or [])!r}\n"
        "[names]\n"
        f"pattern = '''{pattern}'''\n"
        "[floors]\n"
        f"paths = {floor_paths}\n"
        f"names_in_pattern = {floor_names}\n"
    )


def scratch(tmp: pathlib.Path, *, tracked: dict[str, bytes],
            manifest_text: str | None) -> pathlib.Path:
    """A repository with the guard vendored in, unchanged."""
    root = tmp / "r"
    ci = root / "scripts" / "ci"
    ci.mkdir(parents=True)
    (ci / "internal_stays_internal.py").write_text(GUARD.read_text(encoding="utf-8"),
                                                   encoding="utf-8")
    if manifest_text is not None:
        (root / "docs").mkdir(exist_ok=True)
        (root / "docs" / "PUBLIC_TREE.toml").write_text(manifest_text, encoding="utf-8")

    git(root, "init", "-q", "-b", "master")
    for rel, content in tracked.items():
        f = root / rel
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_bytes(content)
    # The guard reads `git ls-files`, so the state under test is the index and
    # not the working tree -- the same rule as the real tree.
    git(root, "add", "-A")
    git(root, "commit", "-qm", "fixture")
    return root


def run_guard(root: pathlib.Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(root / "scripts" / "ci" / "internal_stays_internal.py"), *args],
        cwd=str(root), capture_output=True, text=True, timeout=120,
        env=sealed_env(cwd=root),
    )


# A tree the guard approves: an internal document and its directory, a source
# file that says nothing, and a test that names an internal document and is
# therefore listed under [internal].files.
GREEN_TRACKED = {
    "private/deadbeef_fixture01.pdf": b"%PDF-1.4\n",
    "corpus/list.tsv": ("cafed00d_fixture02\n").encode(),
    "src/lib.rs": b"// nothing to see\n",
    "docs/guide.md": b"An SDK guide.\n",
    "tests/reads_it.rs": b'const D: &str = "deadbeef_fixture01";\n',
}
GREEN_MANIFEST = manifest(files=["tests/reads_it.rs"])


def main() -> int:
    if not GUARD.is_file():
        print(f"SKIPPED (not a pass): {GUARD} is missing, so there is nothing to test",
              file=sys.stderr)
        return 3

    failures: list[str] = []
    cases = 0

    def expect(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        if ok:
            print(f"  ok    {what}")
        else:
            failures.append(f"{what}: {detail}")
            print(f"  FAIL  {what}", file=sys.stderr)

    def case(*args: str, **kwargs) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as d:
            root = scratch(pathlib.Path(d), **kwargs)
            return run_guard(root, *args)

    def tail(r: subprocess.CompletedProcess[str]) -> str:
        return f"exit {r.returncode}\n{(r.stdout + r.stderr)[-600:]}"

    # --- the control ----------------------------------------------------
    # If this fails every red case below is meaningless: the guard would be
    # red for a reason the case did not plant.
    r = case(tracked=GREEN_TRACKED, manifest_text=GREEN_MANIFEST)
    expect("a tree with nothing exposed passes", r.returncode == 0, tail(r))
    expect("and it says what it counted", "checked 3 file(s)" in r.stdout, tail(r))

    # --- the failure this file exists for -------------------------------
    # A source file naming an internal document. This is exactly the shape
    # that stood red and unseen on master on 05-09-2026.
    r = case(tracked={**GREEN_TRACKED,
                      "src/context.rs": b"// measured on cafed00d_fixture02.pdf\n"},
             manifest_text=GREEN_MANIFEST)
    expect("a published source file naming an internal document fails",
           r.returncode == 1, tail(r))
    expect("and it names the file and the document",
           "src/context.rs" in r.stdout and "cafed00d_fixture02" in r.stdout, tail(r))

    # The name in a file the manifest keeps in-house is not an exposure: that
    # file is not published. Without this the rule would be unusable -- the
    # tests that read the corpus all name it.
    r = case(tracked={**GREEN_TRACKED,
                      "private/notes.md": b"about deadbeef_fixture01\n"},
             manifest_text=GREEN_MANIFEST)
    expect("the same name under an internal path passes", r.returncode == 0, tail(r))

    r = case(tracked={**GREEN_TRACKED,
                      "tests/other.rs": b"// deadbeef_fixture01\n"},
             manifest_text=manifest(files=["tests/reads_it.rs", "tests/other.rs"]))
    expect("a named file listed under [internal].files passes", r.returncode == 0, tail(r))

    # --- the encodings, which is how a name hides -----------------------
    # A UTF-16 file decoded as UTF-8 with errors="replace" becomes a string
    # with a replacement character between every letter and matches nothing,
    # silently, because "replace" cannot raise.
    for enc, bom in (("utf-16-le", b"\xff\xfe"), ("utf-16-be", b"\xfe\xff"),
                     ("utf-32-le", b"\xff\xfe\x00\x00"), ("utf-32-be", b"\x00\x00\xfe\xff")):
        body = "see fixture_input_form here\n".encode(enc)
        blob = body if body.startswith(bom) else bom + body
        r = case(tracked={**GREEN_TRACKED, "docs/note.md": blob}, manifest_text=GREEN_MANIFEST)
        expect(f"a name written in {enc} is found",
               r.returncode == 1 and "fixture_input_form" in r.stdout, tail(r))

    # utf-8 with a byte-order mark, and a file that is not UTF-8 at all.
    r = case(tracked={**GREEN_TRACKED,
                      "docs/note.md": b"\xef\xbb\xbfsee fixture_input_form\n"},
             manifest_text=GREEN_MANIFEST)
    expect("a name in a UTF-8 file with a BOM is found", r.returncode == 1, tail(r))

    r = case(tracked={**GREEN_TRACKED,
                      "docs/note.md": "caf\xe9 fixture_input_form\n".encode("latin-1")},
             manifest_text=GREEN_MANIFEST)
    expect("a name in a latin-1 file is found", r.returncode == 1, tail(r))

    # --- what must NOT fire ---------------------------------------------
    # A binary is not read: a PDF that happens to hold the bytes of its own
    # name is the document itself, not a description of it.
    r = case(tracked={**GREEN_TRACKED, "docs/pic.png": b"\x89PNG\r\n fixture_input_form\n"},
             manifest_text=GREEN_MANIFEST)
    expect("a binary suffix is not scanned", r.returncode == 0, tail(r))

    # Word boundaries: a longer identifier that merely contains the name is
    # not that document, and a guard that flags it gets switched off.
    r = case(tracked={**GREEN_TRACKED, "src/x.rs": b"let xfixture_input_formy = 1;\n"},
             manifest_text=GREEN_MANIFEST)
    expect("a name embedded in a longer word does not fire", r.returncode == 0, tail(r))

    # A path that merely starts with the same letters is not inside the
    # internal directory. `private/` ends in a slash for this reason, and
    # without it every path beginning "private" would be silently dropped from
    # what the guard reads.
    r = case(tracked={**GREEN_TRACKED, "privateer/x.md": b"see fixture_input_form\n"},
             manifest_text=GREEN_MANIFEST)
    expect("a path that only shares a prefix is still scanned", r.returncode == 1, tail(r))

    # A file whose NAME holds a byte outside ASCII. `git ls-files` without `-z`
    # C-quotes it, and the quoted string names no file on disk -- so the read
    # fails, the error is swallowed, and the name inside is never looked for.
    # Two tracked files are in that state in the real tree.
    r = case(tracked={**GREEN_TRACKED,
                      "docs/gu\u00efde-\u03b2.md": b"about fixture_input_form\n"},
             manifest_text=GREEN_MANIFEST)
    expect("a name inside a file with a non-ASCII filename is found",
           r.returncode == 1 and "fixture_input_form" in r.stdout, tail(r))

    # --- an internal path that reached the tree -------------------------
    # In --tree mode the guard is handed an assembled directory, and the
    # question changes from "would this be published" to "is it here".
    with tempfile.TemporaryDirectory() as d:
        root = scratch(pathlib.Path(d), tracked=GREEN_TRACKED, manifest_text=GREEN_MANIFEST)
        tree = pathlib.Path(d) / "out"
        (tree / "src").mkdir(parents=True)
        (tree / "src" / "lib.rs").write_bytes(b"// nothing to see\n")
        r = run_guard(root, "--tree", str(tree))
        expect("a clean assembled tree passes", r.returncode == 0, tail(r))

        (tree / "private").mkdir()
        (tree / "private" / "deadbeef_fixture01.pdf").write_bytes(b"%PDF-1.4\n")
        r = run_guard(root, "--tree", str(tree))
        expect("an internal path present in the assembled tree fails",
               r.returncode == 1 and "internal path(s) present" in r.stdout, tail(r))
        expect("and it names the path",
               "private/deadbeef_fixture01.pdf" in r.stdout, tail(r))

        # The assembled tree is scanned for names too, not only for paths. It
        # is the tree that would actually be pushed, so this is the last place
        # a name can be caught.
        (tree / "private" / "deadbeef_fixture01.pdf").unlink()
        (tree / "private").rmdir()
        (tree / "src" / "note.rs").write_bytes(b"// cafed00d_fixture02\n")
        r = run_guard(root, "--tree", str(tree))
        expect("a name in the assembled tree fails even with no internal path",
               r.returncode == 1 and "cafed00d_fixture02" in r.stdout, tail(r))

    r = case("--tree", tracked=GREEN_TRACKED, manifest_text=GREEN_MANIFEST)
    expect("--tree without a directory is an error, not a pass",
           r.returncode == 1 and "needs a directory" in r.stderr, tail(r))

    # --- the floors -----------------------------------------------------
    # A guard reading a shortened list reports success over nothing, and both
    # halves of that have bitten here before: `names_in_pattern` sat in the
    # manifest unread for weeks while it declared 15 against a pattern of 11.
    r = case(tracked=GREEN_TRACKED, manifest_text=manifest(files=["tests/reads_it.rs"],
                                                           floor_paths=9))
    expect("fewer internal paths than the floor is FATAL",
           r.returncode == 1 and "below the floor" in r.stderr, tail(r))

    r = case(tracked=GREEN_TRACKED,
             manifest_text=manifest(files=["tests/reads_it.rs"], floor_names=9))
    expect("a names pattern below its floor is FATAL",
           r.returncode == 1 and "alternative(s)" in r.stderr, tail(r))

    # The shape that silences the whole name check: an empty alternation. It
    # matches every string, so without the floor the guard would report every
    # file as exposed -- and with a floor written against a pattern of one, it
    # never runs at all.
    r = case(tracked=GREEN_TRACKED,
             manifest_text=manifest(files=["tests/reads_it.rs"], pattern=r"\b()\b",
                                    floor_names=3))
    expect("an emptied names pattern is refused by the floor",
           r.returncode == 1 and "below the floor" in r.stderr, tail(r))

    # --- the manifest itself --------------------------------------------
    r = case(tracked=GREEN_TRACKED, manifest_text=None)
    expect("an absent manifest is FATAL, not a pass",
           r.returncode == 1 and "is missing" in r.stderr, tail(r))
    expect("and it says why a missing list is not a green",
           "reports success over nothing" in r.stderr, tail(r))

    print()
    if cases < MINIMUM_CASES:
        print(f"FLOOR: {cases} case(s) ran, fewer than {MINIMUM_CASES}. A case went "
              "missing; this is not a green.", file=sys.stderr)
        return 1
    if failures:
        print(f"{len(failures)} of {cases} case(s) failed:", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print(f"{cases} passed, 0 failed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
