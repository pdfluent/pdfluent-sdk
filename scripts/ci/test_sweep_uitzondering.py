#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""De sweep-uitzondering, en vooral de manieren waarop hij niet mag werken.

Deze code draait een commando dat in een commitbericht staat. Dat is precies het
soort pad dat een test verdient voordat het ergens draait, en geval 2 is de
reden: zonder dat de machtiging uit de BASIS komt, machtigt een tak zichzelf.
"""
from __future__ import annotations
import os, pathlib, shutil, subprocess, sys, tempfile

CI = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(CI))
from fixture_env import sealed_env  # noqa: E402

WACHTER = CI / "territories_do_not_overlap.py"

KAART = '''[[territory]]
id = "t1"
name = "Ander"
paden = ["bezit/**"]

[[territory]]
id = "t3"
name = "Mij"
paden = ["mijn/**"]
'''
SWEEPS = '\n[sweeps]\ntoegestaan = ["python3 scripts/sweep.py"]\n'

# Een sweep die iets echts doet en idempotent is: hij zet een vaste regel
# bovenaan elk bestand in bezit/, en laat hem staan als hij er al is.
SWEEP = '''#!/usr/bin/env python3
import pathlib
for f in sorted(pathlib.Path("bezit").glob("*.txt")):
    t = f.read_text()
    if not t.startswith("# kop\\n"):
        f.write_text("# kop\\n" + t)
'''


def git(*a, cwd, **kw):
    return subprocess.run(["git", *a], cwd=str(cwd), capture_output=True, text=True,
                          env=sealed_env(cwd=cwd), **kw)


def repo(tmp: pathlib.Path, met_allowlist_op_basis: bool) -> pathlib.Path:
    """Een repo met een basis (master) en een tak erop."""
    r = tmp / "repo"
    (r / ".claude").mkdir(parents=True)
    (r / "scripts" / "ci").mkdir(parents=True)
    (r / "bezit").mkdir()
    (r / "mijn").mkdir()
    shutil.copy(WACHTER, r / "scripts" / "ci" / WACHTER.name)
    shutil.copy(CI / "fixture_env.py", r / "scripts" / "ci" / "fixture_env.py")
    (r / "scripts" / "sweep.py").write_text(SWEEP)
    (r / ".claude" / "territories.toml").write_text(
        KAART + (SWEEPS if met_allowlist_op_basis else ""))
    for i in range(3):
        (r / "bezit" / f"a{i}.txt").write_text(f"regel {i}\n")
    (r / "mijn" / "eigen.txt").write_text("van mij\n")
    git("init", "-q", "-b", "master", cwd=r)
    git("add", "-A", cwd=r)
    git("commit", "-q", "-m", "basis", cwd=r)
    git("checkout", "-q", "-b", "t3/sweep", cwd=r)
    return r


def sweep_commit(r: pathlib.Path, commando: str, met_handwerk: str = "") -> None:
    subprocess.run([sys.executable, "scripts/sweep.py"], cwd=str(r), check=True)
    if met_handwerk == "geraakt":
        # De generator beheert de kopregel, dus dit maakt hij ongedaan.
        (r / "bezit" / "a0.txt").write_text("# andere kop\nregel 0\n")
    elif met_handwerk == "ongeraakt":
        # Een bestand dat de generator nooit aanraakt. Herdraaien merkt dit
        # NIET op, en dat is de grens van het mechanisme, geen fout erin.
        (r / "bezit" / "handmatig.md").write_text("dit heeft niemand gegenereerd\n")
    git("add", "-A", cwd=r)
    git("commit", "-q", "-m", f"sweep\n\nGenerated-by: {commando}", cwd=r)


def vraag(r: pathlib.Path) -> tuple[set, str | None]:
    """De wachter in de tijdelijke repo aanroepen, met zijn eigen ROOT."""
    code = ("import sys; sys.path.insert(0, 'scripts/ci');"
            "import territories_do_not_overlap as T;"
            "v, reden = T.sweep_uitzondering('master');"
            "print(len(v)); print(reden or '')")
    uit = subprocess.run([sys.executable, "-c", code], cwd=str(r),
                         capture_output=True, text=True, env=sealed_env(cwd=r))
    if uit.returncode != 0:
        return set(), f"<crash> {uit.stderr.strip()[-200:]}"
    n, reden = uit.stdout.split("\n", 1)
    return set(range(int(n))), (reden.strip() or None)


def geval(naam: str, ok: bool, waarom: str = "") -> bool:
    print(f"  {'ok  ' if ok else 'FOUT'}  {naam}")
    if not ok and waarom:
        print(f"        {waarom}")
    return ok


def main() -> int:
    goed = True

    with tempfile.TemporaryDirectory() as d:
        r = repo(pathlib.Path(d), met_allowlist_op_basis=True)
        sweep_commit(r, "python3 scripts/sweep.py")
        vrij, reden = vraag(r)
        goed &= geval("een toegestane sweep die reproduceert geeft zijn bestanden vrij",
                      reden is None and len(vrij) == 3, f"{len(vrij)} vrij, reden={reden}")

    # HET geval. De tak zet zijn eigen commando in zijn eigen kaart. Als de
    # allowlist uit de werkboom kwam, machtigde de tak zichzelf -- en deze
    # wachter draait vóór elke review, dus "de reviewer ziet het erbij staan"
    # komt te laat.
    with tempfile.TemporaryDirectory() as d:
        r = repo(pathlib.Path(d), met_allowlist_op_basis=False)
        kaart = r / ".claude" / "territories.toml"
        kaart.write_text(kaart.read_text() + SWEEPS)
        git("add", "-A", cwd=r)
        git("commit", "-q", "-m", "de tak machtigt zichzelf", cwd=r)
        sweep_commit(r, "python3 scripts/sweep.py")
        vrij, reden = vraag(r)
        goed &= geval("een tak kan zichzelf niet machtigen via zijn eigen kaart",
                      not vrij, f"{len(vrij)} bestanden vrijgegeven, reden={reden}")

    with tempfile.TemporaryDirectory() as d:
        r = repo(pathlib.Path(d), met_allowlist_op_basis=True)
        sweep_commit(r, "python3 scripts/iets_anders.py")
        vrij, reden = vraag(r)
        goed &= geval("een commando buiten de allowlist wordt geweigerd",
                      not vrij and reden and "toegestaan" in reden, f"reden={reden}")

    with tempfile.TemporaryDirectory() as d:
        r = repo(pathlib.Path(d), met_allowlist_op_basis=True)
        sweep_commit(r, "python3 scripts/sweep.py", met_handwerk="geraakt")
        vrij, reden = vraag(r)
        goed &= geval("handwerk in wat de generator beheert wordt gevangen",
                      not vrij and reden and "niet puur machinaal" in reden, f"reden={reden}")

    with tempfile.TemporaryDirectory() as d:
        r = repo(pathlib.Path(d), met_allowlist_op_basis=True)
        sweep_commit(r, "python3 scripts/sweep.py")
        # Een GEVOLGD bestand met onopgeslagen wijzigingen: dat is wat een
        # herdraai plus `git checkout -- .` zou weggooien.
        (r / "mijn" / "eigen.txt").write_text("werk dat nog nergens anders staat\n")
        vrij, reden = vraag(r)
        goed &= geval("een vuile werkboom is SKIPPED, niet groen en niet opgeruimd",
                      not vrij and reden and "SKIPPED" in reden, f"reden={reden}")
        goed &= geval("het onopgeslagen werk is niet weggepoetst",
                      (r / "mijn" / "eigen.txt").read_text().startswith("werk dat"), "")

    # Dit WAS de bekende grens: "herdraaien geeft een lege diff" bewijst dat het
    # gegenereerde gegenereerd is, maar niet dat er niets anders meereed. Een
    # bestand dat de generator nooit aanraakt reproduceert immers schoon.
    #
    # T2 wees op #1700 aan dat die grens te sluiten is door de vrijstelling door
    # te snijden met wat de generator SCHRIJFT -- gemeten door hem op de ouder van
    # de sweepcommit te draaien. Dat is nu gedrag in plaats van een voetnoot, en
    # dit geval bewaakt het.
    with tempfile.TemporaryDirectory() as d:
        r = repo(pathlib.Path(d), met_allowlist_op_basis=True)
        sweep_commit(r, "python3 scripts/sweep.py", met_handwerk="ongeraakt")
        vrij, reden = vraag(r)
        goed &= geval("handwerk buiten het bereik van de generator wordt gevangen",
                      not vrij and reden and "met de hand" in reden,
                      f"{len(vrij)} vrij, reden={reden}")

    print("test_sweep_uitzondering: " + ("OK" if goed else "GEFAALD"))
    return 0 if goed else 1


if __name__ == "__main__":
    sys.exit(main())
