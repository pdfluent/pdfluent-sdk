#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Does the document register still bite, in both directions?

`every_document_is_registered.py` stood red on master from 02-09-2026 with
seven `[internal].files` entries naming documents that were not tracked, and
nobody saw it, because nothing ran it. A guard that nothing runs and a guard
that has stopped recognising anything look the same from the outside: green,
or absent. This file is the half that can be checked from inside.

Every case builds a throwaway git repository in a temporary directory -- a
manifest, a handful of tracked "documents", a stub provenance register -- and
runs a copy of the real guard inside it. The copy has its floor rewritten to
the size of the fixture; nothing else in it is changed, so a case passes or
fails on the guard's own logic and never on the shape of the real tree.

The provenance register is a stub on purpose. The real `corpus_herkomst.py` is
a table of this repository's own documents; importing it would make every case
depend on what happens to be tracked today.

Exit codes:
  0  every case passed
  1  a case failed, or fewer cases ran than the floor
  3  cannot check (announced, never silent)
"""
from __future__ import annotations

import pathlib
import re
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
GUARD = HERE / "every_document_is_registered.py"
sys.path.insert(0, str(HERE))
from fixture_env import sealed_env  # noqa: E402

MINIMUM_CASES = 24  # FLOOR: fewer means a case went missing, not that all is well

PDF = b"%PDF-1.4\n%fixture\n"


def git(root: pathlib.Path, *args: str) -> None:
    subprocess.run(["git", *args], cwd=str(root), check=True,
                   capture_output=True, text=True,
                   env=sealed_env(identity=True, cwd=root))


def manifest(*, files: list[str] = (), documents: dict[str, str] | None = None,
             paths: list[str] = ("private/",)) -> str:
    docs = "".join(f'"{k}" = "{v}"\n' for k, v in (documents or {}).items())
    return (
        "[internal]\n"
        f"paths = {list(paths)!r}\n"
        f"files = {list(files)!r}\n"
        "[documents]\n" + docs
    )


def scratch(tmp: pathlib.Path, *, tracked: dict[str, bytes],
            provenance: list[str], manifest_text: str | None,
            floor: int = 3) -> pathlib.Path:
    """A repository with the guard vendored in, or raise if the vendoring is stale."""
    root = tmp / "r"
    ci = root / "scripts" / "ci"
    ci.mkdir(parents=True)

    source = GUARD.read_text(encoding="utf-8")
    rewritten, n = re.subn(r"^MIN_DOCUMENTEN = \d+$", f"MIN_DOCUMENTEN = {floor}",
                           source, flags=re.M)
    if n != 1:
        raise RuntimeError("the guard no longer carries `MIN_DOCUMENTEN = <n>` on a "
                           "line of its own; this fixture cannot lower the floor and "
                           "every case below would fail on it")
    (ci / "every_document_is_registered.py").write_text(rewritten, encoding="utf-8")
    (ci / "corpus_herkomst.py").write_text(
        "# stub: the shape every_document_is_registered.py imports, nothing more\n"
        f"DERDEN = {{p: ('someone', 'a fixture', 'ja') for p in {sorted(provenance)!r}}}\n"
        "EIGEN_LOS = {}\n"
        "def herkomst(pad):\n"
        "    return DERDEN.get(pad)\n",
        encoding="utf-8",
    )
    if manifest_text is not None:
        (root / "docs").mkdir()
        (root / "docs" / "PUBLIC_TREE.toml").write_text(manifest_text, encoding="utf-8")

    git(root, "init", "-q", "-b", "master")
    for rel, content in tracked.items():
        f = root / rel
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_bytes(content)
    # The guard reads `git ls-files`, so the state under test is the index,
    # not the working tree. Everything the fixture wrote is tracked; nothing
    # that is only on disk counts, which is the same rule as the real tree.
    git(root, "add", "-A")
    git(root, "commit", "-qm", "fixture")
    return root


def run_guard(root: pathlib.Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(root / "scripts" / "ci" / "every_document_is_registered.py")],
        cwd=str(root), capture_output=True, text=True, timeout=120,
        env=sealed_env(cwd=root),
    )


# A tree in the shape the guard approves: two internal PDFs and one public
# PDF, all with provenance; a Word file kept internal; a test that names an
# internal document, listed under [internal].files and tracked.
GREEN_TRACKED = {
    "private/a.pdf": PDF,
    "private/b.pdf": PDF,
    "public/c.pdf": PDF,
    "private/brief.docx": b"PK\x03\x04 not really a docx\n",
    "tests/reads_a.rs": b"// reads private/a.pdf\n",
}
GREEN_PROVENANCE = ["private/a.pdf", "private/b.pdf", "public/c.pdf"]
GREEN_MANIFEST = manifest(files=["tests/reads_a.rs"])


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

    def case(**kwargs) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as d:
            root = scratch(pathlib.Path(d), **kwargs)
            return run_guard(root)

    def tail(r: subprocess.CompletedProcess[str]) -> str:
        return f"exit {r.returncode}\n{(r.stdout + r.stderr)[-600:]}"

    # --- the control: a registered tree passes -------------------------
    # If this fails, every red case below is meaningless: the guard would be
    # red for a reason the case did not plant.
    r = case(tracked=GREEN_TRACKED, provenance=GREEN_PROVENANCE, manifest_text=GREEN_MANIFEST)
    expect("a fully registered tree passes", r.returncode == 0, tail(r))
    expect("and it reports what it counted", "4 tracked documents" in r.stdout, tail(r))

    # --- #215: a binary in the test-material directories counts as a document
    # A suffix list only catches the extensions somebody thought of. #215 asks
    # the gate to fail on any new binary test file without a registration, and a
    # document does not become safe by being called `.bin`.
    r = case(tracked={**GREEN_TRACKED, "fixtures/thing.bin": b"\x00\x01binary"},
             provenance=GREEN_PROVENANCE, manifest_text=GREEN_MANIFEST)
    expect("an unregistered binary under fixtures/ fails", r.returncode == 1, tail(r))
    expect("and it names the file", "fixtures/thing.bin" in r.stderr, tail(r))

    # And the other direction, which is what keeps the rule usable: a text file
    # in the same directory is not a document. Without this the rule would drag
    # in every .txt and .json fixture and nobody would keep it.
    r = case(tracked={**GREEN_TRACKED, "fixtures/thing.txt": b"just text\n"},
             provenance=GREEN_PROVENANCE, manifest_text=GREEN_MANIFEST)
    expect("a text file under fixtures/ is not a document", r.returncode == 0, tail(r))

    # --- the failure this file exists for: a registration naming nothing --
    # Seven of these stood on master. The entry is under [internal].files, the
    # file is not tracked, and the guard has to say which one.
    r = case(tracked=GREEN_TRACKED, provenance=GREEN_PROVENANCE,
             manifest_text=manifest(files=["tests/reads_a.rs", "docs/decisions/ghost.md"]))
    expect("an [internal].files entry naming an untracked file fails", r.returncode == 1, tail(r))
    expect("and it names the entry", "docs/decisions/ghost.md" in r.stderr, tail(r))
    expect("and says that the file is not tracked", "which is not tracked" in r.stderr, tail(r))

    # The same file on disk but not in the index is still not tracked. This is
    # the shape a forgotten `git add` produces, and the shape a `.gitignore`d
    # document produces, and both have to read as absent.
    with tempfile.TemporaryDirectory() as d:
        root = scratch(pathlib.Path(d), tracked=GREEN_TRACKED, provenance=GREEN_PROVENANCE,
                       manifest_text=manifest(files=["tests/reads_a.rs", "docs/decisions/ghost.md"]))
        (root / "docs" / "decisions").mkdir()
        (root / "docs" / "decisions" / "ghost.md").write_text("on disk only\n")
        r = run_guard(root)
    expect("an entry whose file is on disk but not tracked still fails",
           r.returncode == 1 and "docs/decisions/ghost.md" in r.stderr, tail(r))

    # A [documents] entry naming nothing is the same lie in the other table.
    r = case(tracked=GREEN_TRACKED, provenance=GREEN_PROVENANCE,
             manifest_text=manifest(files=["tests/reads_a.rs"],
                                    documents={"public/gone.docx": "shipped once"}))
    expect("a [documents] entry naming an untracked file fails",
           r.returncode == 1 and "public/gone.docx" in r.stderr, tail(r))

    # And a provenance line for a PDF that is gone -- the sample.pdf case.
    r = case(tracked=GREEN_TRACKED, provenance=GREEN_PROVENANCE + ["public/deleted.pdf"],
             manifest_text=GREEN_MANIFEST)
    expect("a provenance entry for an untracked PDF fails",
           r.returncode == 1 and "public/deleted.pdf" in r.stderr
           and "corpus_herkomst.py registers" in r.stderr, tail(r))

    # --- the first direction: a document nobody registered -------------
    r = case(tracked=GREEN_TRACKED, provenance=["private/a.pdf", "private/b.pdf"],
             manifest_text=GREEN_MANIFEST)
    expect("a PDF without provenance fails",
           r.returncode == 1 and "public/c.pdf: no provenance" in r.stderr, tail(r))

    # A Word file in the published tree, on no list. This is the #215 hole:
    # not a PDF, so the provenance register never sees it.
    tracked = dict(GREEN_TRACKED, **{"public/briefing.docx": b"PK\x03\x04\n"})
    r = case(tracked=tracked, provenance=GREEN_PROVENANCE, manifest_text=GREEN_MANIFEST)
    expect("a non-PDF document reaching the published tree fails",
           r.returncode == 1 and "public/briefing.docx" in r.stderr
           and "lets it reach the published tree" in r.stderr, tail(r))

    # The two ways out: keep it in-house, or say why it may ship.
    r = case(tracked=tracked, provenance=GREEN_PROVENANCE,
             manifest_text=manifest(files=["tests/reads_a.rs", "public/briefing.docx"]))
    expect("the same document listed under [internal].files passes", r.returncode == 0, tail(r))
    r = case(tracked=tracked, provenance=GREEN_PROVENANCE,
             manifest_text=manifest(files=["tests/reads_a.rs"],
                                    documents={"public/briefing.docx": "our own template"}))
    expect("the same document listed under [documents] with a reason passes",
           r.returncode == 0, tail(r))
    expect("and the exception is counted", "1 non-PDF exception" in r.stdout, tail(r))

    # Word's owner file has no document extension and is a document all the
    # same: two bytes of metadata and the name of whoever had it open.
    tracked = dict(GREEN_TRACKED, **{"public/~$briefing.docx": b"\x00\x00owner\n"})
    r = case(tracked=tracked, provenance=GREEN_PROVENANCE, manifest_text=GREEN_MANIFEST)
    expect("a `~$` owner file in the published tree fails",
           r.returncode == 1 and "public/~$briefing.docx" in r.stderr, tail(r))

    # Extension matching is case-insensitive; `.PDF` is a PDF.
    tracked = dict(GREEN_TRACKED, **{"public/SHOUT.PDF": PDF})
    r = case(tracked=tracked, provenance=GREEN_PROVENANCE, manifest_text=GREEN_MANIFEST)
    expect("an upper-case .PDF without provenance fails",
           r.returncode == 1 and "public/SHOUT.PDF" in r.stderr, tail(r))

    # Something under an [internal].paths prefix is out of the published tree
    # by that prefix alone; a non-PDF there needs no entry.
    tracked = dict(GREEN_TRACKED, **{"private/notes.xlsx": b"PK\x03\x04\n"})
    r = case(tracked=tracked, provenance=GREEN_PROVENANCE, manifest_text=GREEN_MANIFEST)
    expect("a non-PDF under an internal path passes without an entry", r.returncode == 0, tail(r))

    # A source file is not a document, whatever it is named.
    tracked = dict(GREEN_TRACKED, **{"public/pdf_writer.rs": b"// .pdf in the name only\n"})
    r = case(tracked=tracked, provenance=GREEN_PROVENANCE, manifest_text=GREEN_MANIFEST)
    expect("a source file whose name mentions pdf is not a document", r.returncode == 0, tail(r))

    # Both directions at once: the guard reports both, not the first it meets.
    r = case(tracked=dict(GREEN_TRACKED, **{"public/briefing.docx": b"PK\n"}),
             provenance=GREEN_PROVENANCE,
             manifest_text=manifest(files=["tests/reads_a.rs", "docs/decisions/ghost.md"]))
    expect("an unregistered document and an orphaned entry are both reported",
           r.returncode == 1 and "public/briefing.docx" in r.stderr
           and "docs/decisions/ghost.md" in r.stderr, tail(r))

    # --- the guard cannot read its manifest: a skip, said out loud --------
    r = case(tracked=GREEN_TRACKED, provenance=GREEN_PROVENANCE, manifest_text=None)
    expect("a missing manifest exits non-zero", r.returncode == 3, tail(r))
    expect("and announces itself as a skip, not a pass",
           "SKIPPED (not a pass)" in r.stderr and "missing" not in r.stdout, tail(r))

    r = case(tracked=GREEN_TRACKED, provenance=GREEN_PROVENANCE,
             manifest_text="[internal\npaths = [\n")
    expect("a manifest that is not TOML exits non-zero", r.returncode == 3, tail(r))
    expect("and announces the skip", "SKIPPED (not a pass)" in r.stderr
           and "not valid TOML" in r.stderr, tail(r))

    r = case(tracked=GREEN_TRACKED, provenance=GREEN_PROVENANCE,
             manifest_text="[floors]\npaths = 4\n")
    expect("a TOML file that is not the manifest exits non-zero as a skip",
           r.returncode == 3 and "SKIPPED (not a pass)" in r.stderr, tail(r))

    # An unreadable manifest must not read as an absent one either way: the
    # word "missing" was the old exit-1 message, and a skip is a different verdict.
    expect("the skip is exit 3, never exit 1 dressed as a finding",
           r.returncode != 1, tail(r))

    # --- the floor: a walk that finds almost nothing is broken -----------
    r = case(tracked=GREEN_TRACKED, provenance=GREEN_PROVENANCE,
             manifest_text=GREEN_MANIFEST, floor=99)
    expect("fewer documents than the floor fails",
           r.returncode == 1 and "fewer than 99" in r.stderr, tail(r))

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
