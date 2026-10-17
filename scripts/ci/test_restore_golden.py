#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for scripts/corpus/{backup,restore}-golden.sh, on throwaway trees.

What is being guarded is a claim, not a script: that the backup of the XFA
golden set can be turned back into the golden set, and that a backup which
cannot be is refused rather than reported as a restore. The set is 2.4 GB on one
disk (#374); the whole value of the copy is that somebody finds out it is broken
before they need it, and that is what these cases are.

Each one backs a small tree up into a directory, breaks exactly one thing about
what is in that directory, and checks the restore refuses and says which thing.

  a file lost                the restore is short and nobody notices
  a file changed             the bytes differ, and only a sha256 sees it
  a file gained              a corpus with an extra document is not the corpus
  the manifest rewritten     the check cannot be the thing that is broken
  the two sides disagree     an rsync that exits 0 has not said the copy is good
  the live set moved on      a backup can be perfectly intact and a year stale
  the source was reachable   a probe the source cannot answer turns every week
                             into the easier of the two questions, silently
  no source at all           the default, and it must not read as a failure:
                             a decision and an outage cannot share a sentence
  a restore over a tree      a restore into an occupied directory is a merge
  a symlink in the source    following one backs up a copy and restores a file

The fourth decides whether any of this is worth having. If the manifest were
only ever checked against the tree it describes, a stick that lost both -- or a
tamper that rewrote both -- would restore clean. metadata.json pins the manifest
by its own sha256, so the restore refuses before it reads it. The mutation that
removes that pin has to fail here, or the pin is decoration.

The fifth is about the copy rather than the backup: the source and the
destination are hashed independently and compared, because rsync's exit code is
a statement about rsync's bookkeeping and not about the bytes on two disks.

Everything runs against `--local`, which is a directory rather than the runner
over ssh. The ssh path differs in one respect -- where the source manifest is
computed -- and the same script computes it either way. A test that needed a
host on a home network to be up would be a test that gets skipped, and a skipped
test is not a gate (#276).
"""

from __future__ import annotations

import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
CORPUS = REPO / "scripts" / "corpus"
GOLDEN = CORPUS / "golden_set.py"
BACKUP = CORPUS / "backup-golden.sh"
RESTORE = CORPUS / "restore-golden.sh"
CHECK = CORPUS / "check-golden.sh"

failures: list[str] = []


def sample_tree(root: pathlib.Path) -> None:
    """A tree with the shapes the golden set has: nesting, binary, empty.

    The document-shaped names are deliberate. The real set is 528 directories
    named by sha256 holding PDFs, PNGs and JSON, so a flat handful of text files
    would not exercise the relative paths the manifest is keyed on.
    """
    (root / "008c709a" / "render").mkdir(parents=True)
    (root / "01bf93cc").mkdir(parents=True)
    (root / "008c709a" / "source.pdf").write_bytes(b"%PDF-1.7\n" + bytes(range(256)) * 8)
    (root / "008c709a" / "render" / "page-1.png").write_bytes(b"\x89PNG\r\n\x1a\n" + os.urandom(2048))
    (root / "008c709a" / "notes.txt").write_text("a document with a note\n", encoding="utf-8")
    (root / "01bf93cc" / "source.pdf").write_bytes(b"%PDF-1.4\n" + os.urandom(4096))
    (root / "01bf93cc" / "metadata.json").write_text(json.dumps({"pages": 3}), encoding="utf-8")
    (root / "01bf93cc" / "empty.txt").write_text("", encoding="utf-8")


def golden(*arguments: str) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, str(GOLDEN), *arguments],
                          capture_output=True, text=True, check=True)


def back_up(tree: pathlib.Path, into: pathlib.Path) -> subprocess.CompletedProcess:
    return subprocess.run(["bash", str(BACKUP), "--local", str(tree), "--into", str(into)],
                          capture_output=True, text=True, cwd=REPO)


def restore(source: pathlib.Path, target: pathlib.Path) -> subprocess.CompletedProcess:
    return subprocess.run(["bash", str(RESTORE), "--from", str(source), "--target", str(target)],
                          capture_output=True, text=True, cwd=REPO)


def check_backup(source: pathlib.Path, *against: str) -> subprocess.CompletedProcess:
    return subprocess.run(["bash", str(CHECK), "--from", str(source), *against],
                          capture_output=True, text=True, cwd=REPO)


def check(name: str, condition: bool, detail: str = "") -> None:
    if not condition:
        failures.append(f"{name}: {detail}")


def case_round_trip(area: pathlib.Path) -> None:
    """The claim itself: what was backed up comes back, file for file."""
    tree, backup = area / "tree", area / "backup"
    tree.mkdir()
    sample_tree(tree)

    done_backup = back_up(tree, backup)
    check("a backup verifies itself", done_backup.returncode == 0, done_backup.stderr[-800:])

    target = area / "restored"
    done = restore(backup, target)
    check("a restore of an intact backup succeeds", done.returncode == 0, done.stderr[-800:])

    original = {p.relative_to(tree): p.read_bytes() for p in tree.rglob("*") if p.is_file()}
    got = {p.relative_to(target): p.read_bytes() for p in target.rglob("*") if p.is_file()}
    check("the restored tree is the tree", original == got,
          f"{len(original)} backed up, {len(got)} restored; "
          f"differing: {sorted(str(k) for k in set(original) ^ set(got))}")

    metadata = json.loads((backup / "metadata.json").read_text())
    check("metadata.json counts the files", metadata["files"] == len(original),
          f"metadata says {metadata['files']}, the tree has {len(original)}")
    check("metadata.json counts the bytes",
          metadata["bytes"] == sum(len(v) for v in original.values()),
          f"metadata says {metadata['bytes']}")

    check("the backup compares the copy against the source",
          "both sides agree" in done_backup.stdout,
          f"the backup did not compare the two manifests: {done_backup.stdout[-400:]}")
    check("the backup restores itself before calling itself a backup",
          "and verified" in done_backup.stdout,
          f"the backup did not rehearse a restore: {done_backup.stdout[-400:]}")

    check("the backup is readable without this repository",
          (backup / "manifest.sha256").read_text().splitlines()[0].split("  ")[0].__len__() == 64,
          "the manifest is not in sha256sum's format")


def case_a_file_lost(area: pathlib.Path) -> None:
    """A document gone from the backup after it was made."""
    tree, backup = area / "tree", area / "backup"
    tree.mkdir()
    sample_tree(tree)
    back_up(tree, backup)

    (backup / "xfa-golden" / "008c709a" / "notes.txt").unlink()

    done = restore(backup, area / "restored")
    check("a file lost from the backup is refused", done.returncode != 0,
          "the restore succeeded with a document missing")
    check("a file lost from the backup is named", "MISSING" in done.stderr,
          f"stderr did not say MISSING: {done.stderr[-400:]}")


def case_a_file_changed(area: pathlib.Path) -> None:
    """One byte of one document flipped. The size is unchanged; only sha256 sees it."""
    tree, backup = area / "tree", area / "backup"
    tree.mkdir()
    sample_tree(tree)
    back_up(tree, backup)

    document = backup / "xfa-golden" / "01bf93cc" / "source.pdf"
    before = document.stat().st_size
    corrupted = bytearray(document.read_bytes())
    corrupted[100] ^= 0xFF
    document.write_bytes(bytes(corrupted))
    check("the mutation did not change the size", document.stat().st_size == before,
          "the case would pass on a size check alone")

    done = restore(backup, area / "restored")
    check("a changed file is refused", done.returncode != 0,
          "the restore succeeded with a corrupted document")
    check("a changed file is named", "CHANGED" in done.stderr,
          f"stderr did not say CHANGED: {done.stderr[-400:]}")


def case_a_file_gained(area: pathlib.Path) -> None:
    """A document nobody listed. `sha256sum -c` has no opinion; this must."""
    tree, backup = area / "tree", area / "backup"
    tree.mkdir()
    sample_tree(tree)
    back_up(tree, backup)

    (backup / "xfa-golden" / "01bf93cc" / "extra.pdf").write_bytes(b"%PDF-1.4\nnobody asked for this\n")

    done = restore(backup, area / "restored")
    check("a file nobody listed is refused", done.returncode != 0,
          "the restore succeeded with an extra document")
    check("a file nobody listed is named", "EXTRA" in done.stderr,
          f"stderr did not say EXTRA: {done.stderr[-400:]}")


def case_the_manifest_was_rewritten_to_agree(area: pathlib.Path) -> None:
    """Tree and manifest changed together -- metadata.json is what catches it.

    Deliberately shaped so that every other check passes: the manifest agrees
    with the tree exactly, the line count agrees with metadata.json, and file by
    file the tree verifies. The only thing left that disagrees is the sha256
    metadata.json holds for the manifest. Remove that check and this case is the
    one that stops failing.
    """
    tree, backup = area / "tree", area / "backup"
    tree.mkdir()
    sample_tree(tree)
    back_up(tree, backup)

    (backup / "xfa-golden" / "01bf93cc" / "source.pdf").write_bytes(b"%PDF-1.4\nsomething else entirely\n")
    golden("manifest", "--root", str(backup / "xfa-golden"), "--out", str(backup / "manifest.sha256"))
    # metadata.json is NOT repinned: it is the record of what was backed up, and
    # rewriting it is the one further step a tamper would have to take.

    intact = json.loads((backup / "metadata.json").read_text())
    check("the rewritten manifest still has the same number of lines",
          intact["files"] == len((backup / "manifest.sha256").read_text().splitlines()),
          "the case would pass on the line-count check alone")

    done = restore(backup, area / "restored")
    check("a manifest rewritten to agree is refused", done.returncode != 0,
          "the restore accepted a manifest that metadata.json does not name")
    check("a manifest rewritten to agree is explained",
          "not the one metadata.json names" in done.stdout + done.stderr,
          f"output did not explain: {(done.stdout + done.stderr)[-400:]}")


def case_the_two_sides_disagree(area: pathlib.Path) -> None:
    """The copy is not the source. The backup must refuse to be made at all.

    `compare` is the only check that says anything about the transfer: it is
    given a manifest of the source and a manifest of the destination, computed
    separately. Here the source manifest is edited to name a document that never
    arrived, which is the shape of a copy that dropped a file silently.
    """
    tree, backup = area / "tree", area / "backup"
    tree.mkdir()
    sample_tree(tree)

    one, other = area / "one.sha256", area / "other.sha256"
    golden("manifest", "--root", str(tree), "--out", str(one))
    lines = one.read_text().splitlines()
    other.write_text("\n".join(lines[:-1]) + "\n")

    def compare(left: pathlib.Path, right: pathlib.Path) -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, str(GOLDEN), "compare", "--one", str(left), "--other", str(right)],
            capture_output=True, text=True)

    done = compare(one, other)
    check("a destination missing a file is refused", done.returncode != 0,
          "compare accepted a destination that is missing a file")
    check("a destination missing a file names it", "ONLY IN" in done.stderr,
          f"stderr did not say ONLY IN: {done.stderr[-400:]}")

    # The other direction. A copy that has gained a file is the same class of
    # wrong, and the two halves of the comparison are separate code.
    done = compare(other, one)
    check("a destination with a file too many is refused", done.returncode != 0,
          "compare accepted a destination holding a file the source does not")
    check("a destination with a file too many names it", "ONLY IN" in done.stderr,
          f"stderr did not say ONLY IN: {done.stderr[-400:]}")

    # Same names on both sides, different bytes -- the shape of a transfer that
    # truncated a document rather than dropping it.
    same_names = area / "same-names.sha256"
    lines = one.read_text().splitlines()
    flipped = ("0" if lines[0][0] != "0" else "1") + lines[0][1:]
    same_names.write_text("\n".join([flipped] + lines[1:]) + "\n")
    done = compare(one, same_names)
    check("two sides with the same names and different bytes are refused", done.returncode != 0,
          "compare accepted a destination whose file has different contents")
    check("differing contents are named", "DIFFERS" in done.stderr,
          f"stderr did not say DIFFERS: {done.stderr[-400:]}")

    done = compare(one, one)
    check("two manifests that agree are accepted", done.returncode == 0, done.stderr[-400:])


def case_a_restore_over_an_occupied_directory(area: pathlib.Path) -> None:
    """A restore into a directory that already holds something is refused.

    rsync without --delete would leave whatever was there, and the verify that
    follows would then be reading a mixture of two sets. Refusing up front is
    the difference between a restore and a merge nobody asked for.
    """
    tree, backup = area / "tree", area / "backup"
    tree.mkdir()
    sample_tree(tree)
    back_up(tree, backup)

    target = area / "occupied"
    target.mkdir()
    (target / "left-over.pdf").write_bytes(b"%PDF-1.4\nfrom something else\n")

    done = restore(backup, target)
    check("a restore over an occupied directory is refused", done.returncode != 0,
          "the restore wrote into a directory that already held a file")
    check("a restore over an occupied directory says why", "not empty" in done.stderr,
          f"stderr did not say the target was not empty: {done.stderr[-400:]}")
    check("a refused restore leaves the directory alone",
          {p.name for p in target.iterdir()} == {"left-over.pdf"},
          f"the target now holds {sorted(p.name for p in target.iterdir())}")


def case_a_symlink_in_the_source(area: pathlib.Path) -> None:
    """A link is refused rather than followed.

    The measured set has none, and that is exactly why this has to be enforced
    rather than assumed: the first link somebody adds would be silently backed
    up as a second copy of its target and restored as a file, and the manifest
    would agree with itself the whole way. Cheaper to refuse than to explain.
    """
    tree, backup = area / "tree", area / "backup"
    tree.mkdir()
    sample_tree(tree)
    (tree / "01bf93cc" / "shortcut.pdf").symlink_to(tree / "008c709a" / "source.pdf")

    done = back_up(tree, backup)
    check("a symlink in the source is refused", done.returncode != 0,
          "the backup followed a symlink instead of refusing it")
    check("a symlink in the source is named", "refusing a symlink" in done.stderr,
          f"stderr did not say which: {done.stderr[-400:]}")


def case_the_weekly_check_sees_the_set_move(area: pathlib.Path) -> None:
    """The backup is intact and no longer the set. Only a source can say so.

    This is the failure the intact check cannot reach: every file in the backup
    is exactly the file that was put there, and the live set has gained a
    document since. A weekly check that only restored the backup would report
    healthy for as long as the stick survives.
    """
    tree, backup = area / "tree", area / "backup"
    tree.mkdir()
    sample_tree(tree)
    back_up(tree, backup)

    done = check_backup(backup, "--local", str(tree))
    check("an unchanged set passes the weekly check", done.returncode == 0, done.stderr[-600:])

    (tree / "008c709a" / "page-2.png").write_bytes(b"\x89PNG\r\n\x1a\n" + os.urandom(64))
    done = check_backup(backup, "--local", str(tree))
    check("a set that has moved on is reported", done.returncode != 0,
          "the weekly check passed a backup that no longer holds the live set")
    check("a set that has moved on names the file", "page-2.png" in done.stderr,
          f"stderr did not name the new document: {done.stderr[-400:]}")


def case_the_weekly_check_sees_a_rotten_backup(area: pathlib.Path) -> None:
    """A backup that has lost a file, checked with no source in the room."""
    tree, backup = area / "tree", area / "backup"
    tree.mkdir()
    sample_tree(tree)
    back_up(tree, backup)

    done = check_backup(backup)
    check("an intact backup passes with no source", done.returncode == 0, done.stderr[-600:])
    check("a check with no source says what it did not claim",
          "nothing is claimed" in done.stdout,
          f"stdout did not say the source was not consulted: {done.stdout[-400:]}")

    (backup / "xfa-golden" / "008c709a" / "notes.txt").unlink()
    done = check_backup(backup)
    check("a rotten backup is caught with no source", done.returncode != 0,
          "the weekly check passed a backup that has lost a document")
    check("a rotten backup is named", "MISSING" in done.stderr,
          f"stderr did not say MISSING: {done.stderr[-400:]}")


def case_the_weekly_probe_asks_a_question_the_source_can_answer(area: pathlib.Path) -> None:
    """The wrapper decides which of its two questions to answer. It can be wrong.

    `golden-weekly.sh` checks whether the source is reachable and, if it is not,
    falls back to the intact check and says so. That fallback is correct and it
    is also the quiet one: a wrapper that always takes it reports a healthy
    backup every week without ever asking whether the live set has moved.

    Which is what it did. The probe was `ssh host true`, and the login shell on
    the machine holding the golden set is PowerShell, where `true` is not a
    command -- so the probe failed against a source that was up, answering, and
    two commands away from a full comparison.

    No host and no network here: `ssh` is a stub on a built PATH that behaves
    the way that machine does. What is checked is the branch the wrapper took.
    """
    tree, backup = area / "tree", area / "backup"
    tree.mkdir()
    sample_tree(tree)
    back_up(tree, backup)

    binaries = area / "bin"
    binaries.mkdir()
    for needed in ("bash", "date", "tee", "mktemp", "rm", "cat", "python3", "sed", "cp"):
        found = shutil.which(needed)
        if found:
            (binaries / needed).symlink_to(found)

    # A login shell that refuses `true` and runs anything else, like the real
    # one. The manifest request is served from the local tree, because what is
    # under test is the decision and not the transport.
    stub = binaries / "ssh"
    stub.write_text(
        "#!/usr/bin/env bash\n"
        'for word in "$@"; do :; done\n'
        'if [ "$word" = "true" ]; then\n'
        '  echo "true : The term \'true\' is not recognized" >&2\n'
        '  exit 1\n'
        "fi\n"
        f'exec python3 {GOLDEN} manifest --root {tree} --out - 2>/dev/null\n',
        encoding="utf-8",
    )
    stub.chmod(0o755)

    environment = dict(os.environ)
    environment["PATH"] = f"{binaries}:{environment['PATH']}"
    environment["GOLDEN_BACKUP"] = str(backup)
    environment["GOLDEN_SOURCE"] = "someone@somewhere:/a/golden/set"
    environment["GOLDEN_LOG"] = str(area / "weekly.log")
    done = subprocess.run(
        ["bash", str(CORPUS / "golden-weekly.sh")],
        capture_output=True, text=True, env=environment,
    )

    check("a reachable source is not reported as unreachable",
          "the source did not answer" not in done.stdout,
          "the weekly check called a source that answers unreachable, and "
          f"answered the easier of its two questions: {done.stdout[-500:]}")
    check("a reachable source is compared against",
          "checking the backup against the live set" in done.stdout,
          f"the weekly check never compared: {done.stdout[-500:]}")
    check("the weekly check passes on a backup that matches the source",
          done.returncode == 0, done.stderr[-600:])


def case_no_source_is_a_decision_and_says_so(area: pathlib.Path) -> None:
    """The scheduled run has no source, and the log has to say why.

    Comparing against the live set means asking the machine that holds it to
    sha256 the whole 2.4 GB. That machine is the CI runner, and a weekly hash
    of the set competes with every job on it -- on 07-09-2026 it did, until the
    runner went offline. So the schedule answers the question it can answer by
    itself, and the runner copy is compared elsewhere or not at all.

    Which leaves a reporting problem, and it is the one this file is about. "No
    source configured" and "the source did not answer" were one branch, so the
    deliberate default printed the sentence an outage prints. Somebody reading
    the log a month later cannot tell a decision from a machine that has been
    down since March. They are separate branches now, and nothing here reaches
    for a network: the case simply runs the wrapper with no GOLDEN_SOURCE.
    """
    tree, backup = area / "tree", area / "backup"
    tree.mkdir()
    sample_tree(tree)
    back_up(tree, backup)

    environment = dict(os.environ)
    environment.pop("GOLDEN_SOURCE", None)
    environment["GOLDEN_BACKUP"] = str(backup)
    environment["GOLDEN_LOG"] = str(area / "weekly.log")
    done = subprocess.run(
        ["bash", str(CORPUS / "golden-weekly.sh")],
        capture_output=True, text=True, env=environment,
    )

    check("a run with no source passes", done.returncode == 0, done.stderr[-600:])
    check("a run with no source calls it a decision",
          "by decision rather than by failure" in done.stdout,
          f"stdout did not distinguish it from an outage: {done.stdout[-500:]}")
    check("a run with no source does not claim the source was tried",
          "did not answer" not in done.stdout,
          "the deliberate default printed the sentence an unreachable source "
          f"prints: {done.stdout[-500:]}")
    check("a run with no source still says what it did not claim",
          "nothing is claimed" in done.stdout,
          f"stdout did not limit its claim: {done.stdout[-500:]}")


def main() -> int:
    for needed in (GOLDEN, BACKUP, RESTORE, CHECK):
        if not needed.is_file():
            print(f"[test-golden] FATAL: {needed} is missing", file=sys.stderr)
            return 1

    for case in (
        case_round_trip,
        case_a_file_lost,
        case_a_file_changed,
        case_a_file_gained,
        case_the_manifest_was_rewritten_to_agree,
        case_the_two_sides_disagree,
        case_a_restore_over_an_occupied_directory,
        case_a_symlink_in_the_source,
        case_the_weekly_check_sees_the_set_move,
        case_the_weekly_check_sees_a_rotten_backup,
        case_the_weekly_probe_asks_a_question_the_source_can_answer,
        case_no_source_is_a_decision_and_says_so,
    ):
        area = pathlib.Path(tempfile.mkdtemp(prefix="golden-case."))
        try:
            case(area)
        finally:
            shutil.rmtree(area, ignore_errors=True)

    if failures:
        print("[test-golden] FATAL:", file=sys.stderr)
        for line in failures:
            print(f"  {line}", file=sys.stderr)
        return 1
    print("[test-golden] OK: a backup checks the copy against the source and restores itself; "
          "a restore refuses a backup that has lost a file, changed a file, gained a file, "
          "or had its manifest rewritten to agree with a changed tree.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
