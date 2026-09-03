#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""De wachter uit #260, en vooral de gevallen waarin hij niet mag falen.

Geval 1 is de opdracht: niet alleen de top van de push. Geval 2 is waarom hij
naar het object kijkt en niet naar de naam. Geval 5 is de reden dat hij bestaat
zonder de SHA te noemen.
"""
from __future__ import annotations
import os, pathlib, shutil, subprocess, sys, tempfile

CI = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(CI))
from fixture_env import sealed_env  # noqa: E402

WACHTER = CI / "geen_verwijderde_blob.py"
INHOUD = b"het weggehaalde document, in deze test een paar bytes\n"


def git(*a, cwd):
    return subprocess.run(["git", *a], cwd=str(cwd), capture_output=True,
                          text=True, env=sealed_env(cwd=cwd))


def bouw(tmp: pathlib.Path) -> tuple[pathlib.Path, str]:
    """Een repo met master, en de blob-SHA van de verboden inhoud."""
    r = tmp / "repo"
    (r / "scripts" / "ci").mkdir(parents=True)
    shutil.copy(WACHTER, r / "scripts" / "ci" / WACHTER.name)
    (r / "leesmij.md").write_text("basis\n", encoding="utf-8")
    git("init", "-q", "-b", "master", cwd=r)
    git("add", "-A", cwd=r)
    git("commit", "-q", "-m", "basis", cwd=r)
    # Een echte bare remote, zoals de sign-off-fixture het ook doet. Zonder
    # `origin/master` valt de wachter terug op "de hele tak", en dan test je de
    # terugval in plaats van het bereik dat een push werkelijk toevoegt.
    kaal = tmp / "kaal.git"
    subprocess.run(["git", "init", "-q", "--bare", str(kaal)], capture_output=True,
                   env=sealed_env(cwd=tmp))
    git("remote", "add", "origin", str(kaal), cwd=r)
    git("push", "-q", "origin", "master", cwd=r)
    sha = subprocess.run(["git", "hash-object", "-w", "--stdin"], cwd=str(r),
                         input=INHOUD, capture_output=True,
                         env=sealed_env(cwd=r)).stdout.decode().strip()
    return r, sha


def draai(r: pathlib.Path, lijst: pathlib.Path | None):
    omg = sealed_env(cwd=r)
    omg["PDFLUENT_VERBODEN_BLOBS"] = str(lijst) if lijst else str(r / "bestaat-niet")
    return subprocess.run([sys.executable, "scripts/ci/geen_verwijderde_blob.py"],
                          cwd=str(r), capture_output=True, text=True, env=omg)


def geval(naam: str, ok: bool, waarom: str = "") -> bool:
    print(f"  {'ok  ' if ok else 'FOUT'}  {naam}")
    if not ok and waarom:
        print(f"        {waarom}")
    return ok


def main() -> int:
    goed = True

    # 1. De blob komt binnen in een commit die NIET de top is, en er komen twee
    #    onschuldige commits overheen. Een wachter die alleen de top bekijkt --
    #    of alleen de werkboom -- ziet hier niets.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        r, sha = bouw(tmp)
        lijst = tmp / "verboden.txt"
        lijst.write_text(f"# een regel commentaar\n{sha}\n", encoding="utf-8")
        git("checkout", "-q", "-b", "t3/werk", cwd=r)
        (r / "formulier.pdf").write_bytes(INHOUD)
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "voegt toe", cwd=r)
        (r / "formulier.pdf").unlink()
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "haalt weer weg", cwd=r)
        (r / "iets.md").write_text("onschuldig\n", encoding="utf-8")
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "iets anders", cwd=r)
        u = draai(r, lijst)
        goed &= geval("een blob in een commit die niet de top is wordt gevangen",
                      u.returncode == 1 and "formulier.pdf" in u.stderr,
                      (u.stdout + u.stderr)[:250])
        # 5. Een treffer mag niet publiceren wat hij bewaakt.
        goed &= geval("een treffer noemt de SHA niet",
                      sha not in (u.stdout + u.stderr),
                      "de SHA staat in de uitvoer")

    # 2. Dezelfde inhoud onder een andere naam. Dit is het verschil met een
    #    naamgebaseerde wachter, en de reden dat #260 om deze vraagt.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        r, sha = bouw(tmp)
        lijst = tmp / "verboden.txt"; lijst.write_text(sha + "\n", encoding="utf-8")
        git("checkout", "-q", "-b", "t3/werk", cwd=r)
        (r / "docs").mkdir()
        (r / "docs" / "een-heel-andere-naam.pdf").write_bytes(INHOUD)
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "hernoemd", cwd=r)
        u = draai(r, lijst)
        goed &= geval("hernoemen helpt niet: het gaat om het object",
                      u.returncode == 1 and "een-heel-andere-naam.pdf" in u.stderr,
                      (u.stdout + u.stderr)[:250])

    # 3. Wat al op master staat is niet wat deze push toevoegt.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        r, sha = bouw(tmp)
        lijst = tmp / "verboden.txt"; lijst.write_text(sha + "\n", encoding="utf-8")
        (r / "al-aanwezig.pdf").write_bytes(INHOUD)
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "stond er al", cwd=r)
        git("push", "-q", "origin", "master", cwd=r)
        git("checkout", "-q", "-b", "t3/werk", cwd=r)
        (r / "nieuw.md").write_text("niets bijzonders\n", encoding="utf-8")
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "gewoon werk", cwd=r)
        u = draai(r, lijst)
        goed &= geval("wat al op master stond is geen bevinding van deze push",
                      u.returncode == 0, (u.stdout + u.stderr)[:250])

    # 4. Geen lijst is geen groen.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        r, _ = bouw(tmp)
        u = draai(r, None)
        goed &= geval("zonder lijst: SKIPPED (not a pass), niet groen",
                      u.returncode != 0 and "SKIPPED (not a pass)" in (u.stdout + u.stderr),
                      (u.stdout + u.stderr)[:250])

    # 6. Een schone tak is groen, en zegt waarover.
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        r, sha = bouw(tmp)
        lijst = tmp / "verboden.txt"; lijst.write_text(sha + "\n", encoding="utf-8")
        git("checkout", "-q", "-b", "t3/werk", cwd=r)
        (r / "gewoon.md").write_text("werk\n", encoding="utf-8")
        git("add", "-A", cwd=r); git("commit", "-q", "-m", "werk", cwd=r)
        u = draai(r, lijst)
        goed &= geval("een schone tak is groen en noemt wat hij bekeek",
                      u.returncode == 0 and "object(en) uit" in u.stdout
                      and "commit(s)" in u.stdout,
                      (u.stdout + u.stderr)[:250])

    print("test_geen_verwijderde_blob: " + ("OK" if goed else "GEFAALD"))
    return 0 if goed else 1


if __name__ == "__main__":
    sys.exit(main())
