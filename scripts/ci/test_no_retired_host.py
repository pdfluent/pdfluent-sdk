#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The retired-host guard has to catch what it claims to catch, and no more.

Every case here is a way the released address, or the name that used to reach
it, has actually sat in this repository: in a comment, in a benchmark report,
in a shell command inside a fenced block, in an ssh config. And the ones that
must NOT fail: a crate of the same name being built over ssh, a live runner
label, another machine's address.

The address is never written in this file either. It is reconstructed from its
octets at the point of use, so the tree stays clean and `no_retired_host.py`
still has something real to find.

Exit codes:
    0  the guard behaves
    1  it let something through, or refused something it should not
"""
from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
GUARD = HERE / "no_retired_host.py"

# Assembled rather than typed, for the same reason the guard holds a digest.
ADDRESS = ".".join(str(n) for n in (46, 225, 223, 175))
NAME = "xfa-test-runner"

# A second address, for the case that says the guard must not fail on every
# machine we ever used. Assembled the same way: this file names ssh, scp and
# rsync in its fixtures, so `no_dead_host_in_a_connecting_script.py` reads it as
# a connecting script and refuses any literal address in it -- correctly, and it
# caught this one before it landed.
OTHER_ADDRESS = ".".join(str(n) for n in (144, 91, 87, 221))

# The floor is 500 files, so a fabricated tree has to be at least that big
# before the guard will judge it at all. That is the floor doing its job, and
# these cases would otherwise all come back as exit 2.
PADDING = 600

cases = 0
failures: list[str] = []


def build(root: pathlib.Path) -> None:
    (root / "filler").mkdir()
    for i in range(PADDING):
        (root / "filler" / f"f{i}.md").write_text("Nothing to see here.\n")
    (root / "scripts").mkdir()
    (root / "scripts" / "build.sh").write_text(
        "#!/usr/bin/env bash\n"
        f'ssh "$VPS" "cargo build --release -p {NAME}"\n'
        f'ssh "$VPS" "cp target/release/{NAME} /usr/local/bin/"\n')
    (root / "runner.yml").write_text(f"runs-on: [self-hosted, {NAME}]\n")
    (root / "other.md").write_text(f"The Contabo box was at {OTHER_ADDRESS}.\n")


def run(root: pathlib.Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(GUARD), "--root", str(root)],
                          capture_output=True, text=True)


def expect(description: str, ok: bool, detail: str = "") -> None:
    global cases
    cases += 1
    print(f"  {'ok   ' if ok else 'FAIL '} {description}")
    if not ok:
        failures.append(description)
        if detail:
            for line in detail.strip().splitlines()[-6:]:
                print(f"          {line}")


def case(write, description, must_fail, needle=""):
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        build(root)
        write(root)
        r = run(root)
        if must_fail:
            ok = r.returncode == 1 and (not needle or needle in r.stderr)
        else:
            ok = r.returncode == 0
        expect(description, ok, r.stdout + r.stderr)
        if must_fail and r.returncode == 1:
            # The guard must not echo what it forbids: a log carrying the
            # address puts it straight back into circulation.
            expect(f"  ... and does not print the address itself",
                   ADDRESS not in (r.stdout + r.stderr))


def main() -> int:
    print("[test-retired-host] the address, in every place it has been found")
    case(lambda root: (root / "note.md").write_text(f"We used {ADDRESS} in May.\n"),
         "a benchmark report naming the address fails", True, "released VPS address")
    case(lambda root: (root / "scripts" / "sync.sh").write_text(
            f"# The host at {ADDRESS} is gone.\n"), 
         "a comment naming the address fails -- that is where it gets copied from",
         True, "released VPS address")
    case(lambda root: (root / "doc.md").write_text(
            f"```bash\nscp report.json root@{ADDRESS}:/tmp/\n```\n"),
         "the address inside a fenced block fails", True)
    case(lambda root: (root / "sshconfig").write_text(
            f"Host old\n    HostName {ADDRESS}\n    StrictHostKeyChecking no\n"),
         "an ssh config pointing at the address fails", True)

    print("[test-retired-host] the name, only where it would dial")
    case(lambda root: (root / "doc.md").write_text(f"Run `ssh {NAME} ls /opt`.\n"),
         "a documented `ssh <name>` fails", True, "would dial")
    case(lambda root: (root / "doc.md").write_text(f"rsync -a out/ {NAME}:/srv/\n"),
         "an rsync target of that name fails", True, "would dial")
    case(lambda root: (root / "doc.md").write_text(f"scp f root@{NAME}:/tmp\n"),
         "a user@name target fails", True, "would dial")
    case(lambda root: (root / "sshconfig").write_text(f"Host {NAME}\n    User root\n"),
         "an ssh-config Host of that name fails", True, "would dial")

    print("[test-retired-host] and what it must leave alone")
    case(lambda root: None,
         "the repaired tree passes: a crate of that name built over ssh, and a "
         "runner label", False)
    case(lambda root: (root / "ci.yml").write_text(
            f"  - name: build\n    run: cargo test -p {NAME} --release\n"),
         "building the crate of the same name still passes", False)
    case(lambda root: (root / "hosts.md").write_text(
            f"The Contabo box answered at {OTHER_ADDRESS}.\n"),
         "another machine's address still passes", False)

    print("[test-retired-host] the floor")
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        (root / "one.md").write_text("alone\n")
        r = run(root)
        expect("a tree too small to have been scanned is FATAL, not a pass",
               r.returncode == 2 and "floor" in r.stderr, r.stdout + r.stderr)

    if failures:
        print(f"\nFAIL: {len(failures)} case(s): {'; '.join(failures)}")
        return 1
    print(f"\nOK: {cases} case(s) -- the address in four places, the name in the "
          f"four shapes that dial it, three things it must not touch, and the floor.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
