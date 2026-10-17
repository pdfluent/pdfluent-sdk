#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The advisory scan must be able to start, and must refuse the wrong bytes.

`cargo-deny check advisories` ran through `EmbarkStudios/cargo-deny-action@v2`,
a Docker container action, on two machines that have no docker: the desktop and
the Hetzner snapshot the wave buys. Measured over the last hundred runs on
master before 07-09-2026: 28 failures, 1 pass, the last of them
`docker: command not found`. Four vulnerabilities sat behind it for months
(#287, #350, #361).

So the job installs the upstream release binary instead, and the two things that
can go wrong with that are covered here by running the installer rather than by
reading it:

  * it must refuse an archive whose digest is not the one this repository
    pinned -- fetching upstream's .sha256 from the host that served the tarball
    proves nothing, so the digest lives in the script;
  * it must refuse a binary that answers with a different version, because
    cargo-deny's verdicts move between releases.

And the workflow half: no step in security-audit.yml may reach for a container
action again, and the cargo-deny step must call the installer.
"""
from __future__ import annotations

import hashlib
import os
import pathlib
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile

import yaml

ROOT = pathlib.Path(__file__).resolve().parents[2]
INSTALLER = ROOT / "scripts/ci/install_cargo_deny.sh"
WORKFLOW = ROOT / ".github/workflows/security-audit.yml"

failures: list[str] = []


def fail(line: str) -> None:
    failures.append(line)


def pinned() -> tuple[str, dict[str, str]]:
    """The version and the per-architecture digests the installer holds."""
    tekst = INSTALLER.read_text()
    versie = re.search(r'^VERSION="([^"]+)"', tekst, re.M)
    digests = dict(re.findall(r'ARCH="(\w+)";\s+SHA256="([0-9a-f]{64})"', tekst))
    return (versie.group(1) if versie else ""), digests


def nep_archief(map_: pathlib.Path, naam: str, versie: str) -> pathlib.Path:
    """A tarball shaped like upstream's: one directory, one `cargo-deny` in it."""
    bin_dir = map_ / naam
    bin_dir.mkdir(parents=True)
    exe = bin_dir / "cargo-deny"
    exe.write_text(f'#!/usr/bin/env bash\necho "cargo-deny {versie}"\n')
    exe.chmod(exe.stat().st_mode | stat.S_IEXEC)
    tgz = map_ / f"{naam}.tar.gz"
    with tarfile.open(tgz, "w:gz") as tar:
        tar.add(bin_dir, arcname=naam)
    return tgz


def installer_voor(werk: pathlib.Path, tgz: pathlib.Path, sha: str) -> pathlib.Path:
    """A copy of the shipped installer pointed at a local archive.

    The URL and the digest are rewritten and NOTHING else, so what runs here is
    the shipped control flow -- an installer with a test-only download switch
    would prove the switch works and not the script.
    """
    werk.mkdir(parents=True, exist_ok=True)
    kopie = werk / "install_cargo_deny.sh"
    tekst = INSTALLER.read_text()
    tekst = re.sub(r'^URL=.*$', f'URL="file://{tgz}"', tekst, count=1, flags=re.M)
    tekst = re.sub(r'SHA256="[0-9a-f]{64}"', f'SHA256="{sha}"', tekst)
    kopie.write_text(tekst)
    kopie.chmod(0o755)
    return kopie


def draai(script: pathlib.Path, dest: pathlib.Path) -> subprocess.CompletedProcess:
    return subprocess.run(["bash", str(script), str(dest)],
                          capture_output=True, text=True)


def main() -> int:
    if not INSTALLER.is_file():
        print(f"[cargo-deny] FAIL: {INSTALLER.relative_to(ROOT)} is gone; the "
              "advisory scan has nothing to install")
        return 1

    versie, digests = pinned()
    if not versie:
        fail("the installer pins no VERSION; cargo-deny's verdicts move between "
             "releases, so an unpinned tool fails this job for reasons that have "
             "nothing to do with our dependency graph")
    if not digests:
        fail("the installer holds no sha256; a download verified against a checksum "
             "fetched from the same host is not verified")

    if shutil.which("curl") is None:
        print("SKIPPED (not a pass): no curl on this machine, so the installer's "
              "download path could not be exercised.", file=sys.stderr)
    else:
        with tempfile.TemporaryDirectory() as tmp:
            werk = pathlib.Path(tmp)
            naam = f"cargo-deny-{versie or '0.0.0'}-{os.uname().machine}-unknown-linux-musl"
            tgz = nep_archief(werk / "goed", naam, versie or "0.0.0")
            echte_sha = hashlib.sha256(tgz.read_bytes()).hexdigest()

            # 1. The happy path. Without it, the refusals below would prove only
            #    that the installer can fail, not that it tells the cases apart.
            out = draai(installer_voor(werk / "a", tgz, echte_sha), werk / "dest-a")
            if out.returncode != 0:
                fail(f"the installer fails on an archive it should accept "
                     f"(rc={out.returncode}): {out.stderr.strip()[:300]}")
            elif not (werk / "dest-a" / "cargo-deny").is_file():
                fail("the installer exited 0 without putting a cargo-deny anywhere")

            # 2. The bytes are not the pinned bytes. This is the case the
            #    container action never reached and the reason the digest is in
            #    the tree rather than fetched beside the tarball.
            out = draai(installer_voor(werk / "b", tgz, "0" * 64), werk / "dest-b")
            if out.returncode == 0:
                fail("the installer accepted an archive whose digest is not the "
                     "pinned one; then the pin is decoration")
            elif (werk / "dest-b" / "cargo-deny").exists():
                fail("the installer refused the archive and installed it anyway")

            # 3. A binary that answers with another version. `cargo install`
            #    upgrades, a reused instance keeps an old copy, and either way a
            #    different cargo-deny grades a different question.
            anders = nep_archief(werk / "oud", naam, "0.0.1-not-the-pin")
            out = draai(installer_voor(werk / "c", anders,
                                       hashlib.sha256(anders.read_bytes()).hexdigest()),
                        werk / "dest-c")
            if out.returncode == 0:
                fail("the installer accepted a binary that reports a version other "
                     "than the pinned one")

    # 4. The workflow half.
    doc = yaml.safe_load(WORKFLOW.read_text())
    stappen = [stap for job in doc.get("jobs", {}).values()
               for stap in (job.get("steps", []) or [])]
    container = [stap.get("uses") for stap in stappen
                 if "cargo-deny" in (stap.get("uses") or "")]
    if container:
        fail(f"security-audit.yml still reaches for {container[0]}; that is a "
             "container action and neither machine this repository runs on has "
             "docker")
    if not any("install_cargo_deny.sh" in (stap.get("run") or "") for stap in stappen):
        fail("no step in security-audit.yml installs cargo-deny; the check would "
             "either not run or run whatever the machine happens to carry")
    if not any(re.search(r"\bcargo[- ]deny\b.*\bcheck\b", stap.get("run") or "")
               for stap in stappen):
        fail("no step in security-audit.yml runs `cargo deny check`; installing it "
             "is not the same as asking it anything")

    if failures:
        print("[cargo-deny] FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1
    print(f"[cargo-deny] OK: the installer takes the pinned {versie} archive, refuses "
          "one whose digest it did not record and one that answers with another "
          "version, and security-audit.yml runs the binary rather than a container.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
