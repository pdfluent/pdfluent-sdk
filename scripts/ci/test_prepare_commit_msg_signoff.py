#!/usr/bin/env python3
"""De haak zet de sign-off, en `format.signoff` doet dat niet.

Fixtures in plaats van beweringen over de broncode, want het onderwerp is wat
git DOET. Dit bestaat omdat een instelling waar iedereen in geloofde niets deed:
`git config format.signoff true` stond hier al aan en geldt alleen voor
`git format-patch`. `git commit` kijkt er niet naar.

Die tweede zaak is de reden dat de haak er is, en zonder deze test staat dat
alleen in een commitboodschap -- waar niets hem tegenhoudt als iemand de haak
ooit vervangt door de config waarvan hij aanneemt dat die werkt. (T1-review.)
"""
from __future__ import annotations
import os, pathlib, subprocess, sys, tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from fixture_env import sealed_env  # noqa: E402

HAAK = pathlib.Path(__file__).resolve().parents[2] / ".githooks" / "prepare-commit-msg"


def schoon(cwd=None) -> dict[str, str]:
    """De verzegelde omgeving voor een fixture die een repository bouwt.

    GIT_* strippen is niet genoeg: de globale en de systeemconfig blijven dan in
    het spel, en `git config user.email` in een fixture schrijft alsnog ergens
    echt. sealed_env sluit alle drie de vlakken en zet met `cwd` het plafond,
    zodat git geen echte repository vindt door omhoog te lopen. Die tweede helft
    is de bevinding uit #1647, en ze geldt net zo goed voor deze test als voor
    de fixtures waar ik haar aanwees.
    """
    return sealed_env(cwd=cwd)


def git(*a: str, cwd: pathlib.Path) -> subprocess.CompletedProcess:
    return subprocess.run(["git", *a], cwd=str(cwd), capture_output=True,
                          text=True, env=schoon(cwd=cwd))


def trailers(repo: pathlib.Path) -> int:
    uit = git("log", "-1", "--format=%B", cwd=repo).stdout
    return sum(1 for r in uit.splitlines() if r.startswith("Signed-off-by:"))


def bouw(tmp: pathlib.Path, met_haak: bool, signoff_config: bool = False,
         naam: str = "") -> pathlib.Path:
    repo = tmp / (naam or ("met" if met_haak else "zonder"))
    repo.mkdir()
    git("init", "-q", ".", cwd=repo)
    git("config", "user.name", "proef", cwd=repo)
    git("config", "user.email", "proef@invalid", cwd=repo)
    if signoff_config:
        git("config", "format.signoff", "true", cwd=repo)
    if met_haak:
        d = repo / ".githooks"
        d.mkdir()
        (d / "prepare-commit-msg").write_bytes(HAAK.read_bytes())
        (d / "prepare-commit-msg").chmod(0o755)
        git("config", "core.hooksPath", ".githooks", cwd=repo)
    return repo


def main() -> int:
    fouten: list[str] = []

    # Een ontbrekende haak is een bevinding, geen traceback. Zonder dit viel de
    # test om met FileNotFoundError, en dat leest als "de test is stuk" terwijl
    # het antwoord juist is dat het onderwerp weg is.
    if not HAAK.is_file():
        print(f"test_prepare_commit_msg_signoff: {HAAK} bestaat niet, dus er is "
              "niets dat de sign-off zet.", file=sys.stderr)
        return 1

    def eis(wat: str, ok: bool, detail: str = "") -> None:
        if not ok:
            fouten.append(f"{wat}{': ' + detail if detail else ''}")

    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)

        # 1. Zonder haak: geen trailer. Zo weet je dat de haak het doet en niet
        #    iets anders in de omgeving.
        zonder = bouw(tmp, met_haak=False)
        git("commit", "-q", "--allow-empty", "-m", "x", cwd=zonder)
        eis("zonder haak hoort er geen sign-off te staan", trailers(zonder) == 0,
            f"{trailers(zonder)} gevonden")

        # 2. `format.signoff` alleen: nog steeds geen trailer. DIT is de reden
        #    dat de haak bestaat, en de enige controle die dat vastlegt.
        alleen_config = bouw(tmp, met_haak=False, signoff_config=True, naam="alleenconfig")
        (alleen_config / "f").write_text("x")
        git("add", "f", cwd=alleen_config)
        git("commit", "-q", "-m", "x", cwd=alleen_config)
        eis("format.signoff alleen voegt niets toe aan `git commit`",
            trailers(alleen_config) == 0,
            f"{trailers(alleen_config)} gevonden -- als dit ooit 1 wordt, is de "
            "haak overbodig geworden en hoort hij weg, niet te blijven staan")

        # 3. Met haak: precies een.
        met = bouw(tmp, met_haak=True)
        git("commit", "-q", "--allow-empty", "-m", "x", cwd=met)
        eis("met de haak staat er een sign-off", trailers(met) == 1,
            f"{trailers(met)} gevonden")

        # 4. Idempotent: amenden en een expliciete -s verdubbelen niets.
        git("commit", "-q", "--amend", "--allow-empty", "--no-edit", cwd=met)
        eis("een --amend verdubbelt de sign-off niet", trailers(met) == 1,
            f"{trailers(met)} gevonden")
        git("commit", "-q", "--allow-empty", "-s", "-m", "met -s", cwd=met)
        eis("een expliciete -s verdubbelt de sign-off niet", trailers(met) == 1,
            f"{trailers(met)} gevonden")

        # 5. Zonder identiteit: geen crash. `set -e` plus een kale
        #    `$(git config user.name)` doodt de haak, en git meldt dan alleen
        #    "hook failed" -- de eigen uitleg bereikt de gebruiker nooit. (T1.)
        kaal = tmp / "kaal"
        kaal.mkdir()
        git("init", "-q", ".", cwd=kaal)
        d2 = kaal / ".githooks"
        d2.mkdir()
        (d2 / "prepare-commit-msg").write_bytes(HAAK.read_bytes())
        (d2 / "prepare-commit-msg").chmod(0o755)
        git("config", "core.hooksPath", ".githooks", cwd=kaal)
        r = subprocess.run(["git", "commit", "--allow-empty", "-m", "x"],
                           cwd=str(kaal), capture_output=True, text=True,
                           env=dict(schoon(cwd=kaal), GIT_AUTHOR_NAME="a", GIT_AUTHOR_EMAIL="a@b",
                                    GIT_COMMITTER_NAME="a", GIT_COMMITTER_EMAIL="a@b"))
        eis("zonder user.name faalt de haak niet", r.returncode == 0,
            f"exit {r.returncode}: {(r.stderr or '').strip()[:120]}")

    if fouten:
        print("test_prepare_commit_msg_signoff: de haak doet niet wat hij belooft.\n",
              file=sys.stderr)
        for f in fouten:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print("test_prepare_commit_msg_signoff: OK -- 6 geval(len); de haak zet de "
          "sign-off, format.signoff niet.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
