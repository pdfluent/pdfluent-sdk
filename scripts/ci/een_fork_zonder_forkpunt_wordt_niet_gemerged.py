#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Elke fork noemt een forkpunt, of zegt hardop dat hij niet gemerged mag worden.

WAAROM DIT BESTAAT

Het register is zes keer fout geweest, elke keer dezelfde kant op: het beweerde
dat we actueler waren dan we waren. Twee daarvan wezen naar een commit *na* een
cherry-pick. Een drieweg-merge op zo'n basis leest upstream-werk dat wij niet
hebben als iets wat wij hebben verwijderd, en draait het terug -- terwijl het
rapport een upgrade meldt. Dat is de duurste vorm, want de uitkomst ziet er goed
uit.

Tegen een verkeerd forkpunt helpt `the_fork_register_is_verifiable.py`: die
controleert of de versie klopt bij de commit die genoemd wordt. Maar tegen een
*ontbrekend* forkpunt helpt niets. Op 31-08-2026 droegen vier van de negen
regels helemaal geen `forkpunt`, alleen een versienummer -- en een versienummer
kan een forkpunt niet zien. Er viel dus niets te controleren, en het gat zag er
identiek uit aan een gecontroleerd gat.

`pdf-render` is het geval waar dit niet met beter meten op te lossen is. Onze
`src/` is twee bestanden tegen upstreams zeven, geen enkele naam gedeeld, dus
geen enkel bestand is byte-identiek aan welke upstream-revisie dan ook en er
zijn geen vensters om te doorsnijden. Het forkpunt is niet onbekend bij gebrek
aan moeite; de methode reikt er niet.

WAT DEZE CONTROLE AFDWINGT

Elke `[[fork]]` draagt precies een van twee dingen:

  forkpunt = "<commit>"     -- hier komt onze code vandaan, controleerbaar
  niet_mergen = "<reden>"   -- geen forkpunt, dus deze crate wordt niet gemerged

Geen van beide is een fout. Allebei ook: dan spreekt de regel zichzelf tegen.

`niet_mergen` is geen ontsnapping maar een rem. Het staat in de uitvoer, het
staat in het register, en het maakt van een gat een besluit in plaats van een
omissie. Een crate met `niet_mergen` mag pas gemerged worden als het veld weg is
-- en het veld gaat pas weg als er een forkpunt voor in de plaats komt.

Exitcodes:
    0  elke fork noemt een forkpunt of een reden om niet te mergen
    1  een fork noemt geen van beide, of allebei
    2  het register ontbreekt of is onleesbaar
"""

from __future__ import annotations

import os
import pathlib
import subprocess
import sys

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python < 3.11
    print("SKIPPED (not a pass): geen tomllib, dus het register is niet gelezen",
          file=sys.stderr)
    raise SystemExit(2)

WORTEL = pathlib.Path(__file__).resolve().parents[2]
LIJST = WORTEL / "docs" / "UPSTREAM_FORKS.toml"

# ONDERGRENS: het register heeft er negen. Vindt deze controle er minder dan
# vijf, dan is het inlezen stuk en niet het bestand leeg -- en een controle die
# nul regels controleert en groen meldt is precies de fout die dit project al
# vier keer heeft gehad.
MINIMAAL_AANTAL_FORKS = 5


def schone_omgeving() -> dict[str, str]:
    """The caller's environment with every GIT_* variable removed.

    A git hook exports `GIT_DIR` and `GIT_WORK_TREE`, and git then works on the
    repository they name and ignores `cwd=` entirely. For a read-only command
    that is merely wrong; for the fetch in the register guards it was
    destructive, because the refspec is a force-update of every branch.

    Reproduced in a throwaway repository: a branch with an unpushed commit on
    top of a pushed one lost that commit outright. What hid it is luck -- git
    refuses to fetch into a branch that is checked out in a worktree and aborts
    the whole fetch, and one of ours always is, which is why this surfaced in
    the logs as `SKIPPED (not a pass)` rather than as damage. A detached HEAD
    has no such protection, and detached HEAD is what `actions/checkout`
    produces and what half of our worktrees are.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def registerregels_die_bewogen() -> set[str] | None:
    """Crates whose `gelijk_met` or `forkpunt` this branch changes.

    NOT "crates whose files changed", which is what the first version asked.
    That blocked any edit at all to a crate carrying `niet_mergen`, and #1543
    showed what that costs: it failed on `cff-parser` because the branch added a
    fork-attribution note to the README and 99 lines of tests. Neither is merging
    a fork forward, and the rule would have blocked routine licence work across
    every forked crate.

    There is no way to tell an upstream merge from added tests by looking at the
    diff -- both are substantive source changes, and I measured that rather than
    assumed it. What an upstream merge does have is a claim: it says the crate now
    corresponds to a newer upstream. That claim lives in `gelijk_met` and
    `forkpunt`, and moving either of them for a crate that has no verifiable fork
    point is the thing worth refusing.

    The other half -- code moved forward while the register stayed still -- is a
    real fault and is NOT this check's to catch. It has an owner:
    `een_forkpunt_wordt_op_inhoud_gecontroleerd.py` scores each fork point by how
    many files are byte-identical to upstream there, and a tree that moved toward
    a newer upstream stops sitting at the maximum. Measured: pointing pdf-syntax
    at `758948489` gives 15 of 47 where 20 is reachable. That guard fails; this
    one is silent about it, on purpose, rather than pretending to a discriminator
    it does not have.
    """
    import tomllib

    def velden(tekst: str) -> dict[str, tuple[str | None, str | None]]:
        try:
            forks = tomllib.loads(tekst)["fork"]
        except Exception:  # noqa: BLE001 - unreadable means "cannot tell"
            return {}
        return {
            f["onze_crate"]: (f.get("gelijk_met"), f.get("forkpunt")) for f in forks
        }

    for ref in ("github/master", "origin/master", "master"):
        basis = subprocess.run(
            ["/usr/bin/git", "merge-base", "HEAD", ref],
            cwd=WORTEL, capture_output=True, text=True, env=schone_omgeving(),
        check=False,
        )
        if basis.returncode != 0:
            continue
        eerder = subprocess.run(
            ["/usr/bin/git", "show", f"{basis.stdout.strip()}:docs/UPSTREAM_FORKS.toml"],
            cwd=WORTEL, capture_output=True, text=True, env=schone_omgeving(),
        check=False,
        )
        if eerder.returncode != 0:
            continue
        toen = velden(eerder.stdout)
        nu = velden(LIJST.read_text())
        if not toen or not nu:
            return set()
        return {c for c, v in nu.items() if c in toen and toen[c] != v}

    # None, not an empty set. An empty set means "no register line moved", which
    # is the answer that lets everything through -- and on a shallow checkout
    # neither github/master nor origin/master nor master exists, so this was the
    # answer on every Actions run. The guard was wired, ran, printed SKIPPED and
    # returned 0 while a change to a `niet_mergen` entry's `gelijk_met` or
    # `forkpunt` passed underneath it. (codex, #1639)
    print("SKIPPED (not a pass): geen merge-base gevonden, dus niet vastgesteld "
          "welke registerregels deze tak beweegt; `niet_mergen` blokkeert hier niets.",
          file=sys.stderr)
    return None


def main() -> int:
    if not LIJST.exists():
        print(f"FAIL: {LIJST.relative_to(WORTEL)} ontbreekt; er is niets dat vastlegt "
              "waar de forks vandaan komen.", file=sys.stderr)
        return 2

    try:
        forks = tomllib.load(LIJST.open("rb"))["fork"]
    except Exception as fout:  # noqa: BLE001 - elke leesfout is hier hetzelfde
        print(f"FAIL: {LIJST.relative_to(WORTEL)} is onleesbaar: {fout}", file=sys.stderr)
        return 2

    if len(forks) < MINIMAAL_AANTAL_FORKS:
        print(f"ONDERGRENS: {len(forks)} fork(s) gelezen, verwacht >= "
              f"{MINIMAAL_AANTAL_FORKS}. Het inlezen is stuk -- dit is geen groen.",
              file=sys.stderr)
        return 1

    fouten: list[str] = []
    geblokkeerd: list[tuple[str, str]] = []
    bewogen = registerregels_die_bewogen()
    if bewogen is None:
        print("[forkpunt] FATAAL: zonder merge-base is niet vast te stellen welke "
              "registerregels deze tak verplaatst, en een lege verzameling zou "
              "betekenen dat er niets bewoog. Haal de basis op (fetch-depth: 0 of "
              "een gerichte fetch van de base-sha) en draai opnieuw.",
              file=sys.stderr)
        return 2

    for f in forks:
        naam = f.get("onze_crate", "<naamloos>")
        forkpunt = f.get("forkpunt")
        niet_mergen = f.get("niet_mergen")

        if forkpunt and niet_mergen:
            fouten.append(
                f"  {naam}: draagt zowel `forkpunt` als `niet_mergen`. Als het "
                "forkpunt bekend is, is er geen reden hem niet te mergen; haal er "
                "een van weg."
            )
        elif not forkpunt and not niet_mergen:
            fouten.append(
                f"  {naam}: geen `forkpunt` en geen `niet_mergen`. Een versienummer "
                "kan een forkpunt niet zien, dus deze regel is niet te controleren "
                "en mergen ertegen is raden."
            )
        elif niet_mergen:
            geblokkeerd.append((naam, niet_mergen))

    for naam, reden in geblokkeerd:
        print(f"[forkpunt] NIET MERGEN: {naam} -- {reden}")
        # Printing it is not blocking it. The step in .github/workflows/ci.yml
        # reads the exit status and nothing else, so a branch that upgraded one
        # of these crates while leaving the field in place passed with a warning
        # in the log that nobody reads. (Codex, #1609.)
        if naam in bewogen:
            fouten.append(
                f"  {naam}: carries `niet_mergen`, and this branch moves its "
                "`gelijk_met` or `forkpunt`. Those fields claim the crate now "
                "corresponds to a particular upstream -- which is exactly what "
                "`niet_mergen` says cannot be established for it. Establish the "
                "fork point by content and replace the field, or leave both alone. "
                "(Editing the crate's files is fine; this is about the claim.)"
            )

    if fouten:
        print(file=sys.stderr)
        print(f"[forkpunt] FAIL: {len(fouten)} regel(s) in "
              f"{LIJST.relative_to(WORTEL)} zonder controleerbaar forkpunt:",
              file=sys.stderr)
        for regel in fouten:
            print(regel, file=sys.stderr)
        print(
            "\nEen fork zonder forkpunt wordt niet gemerged. Stel het forkpunt vast "
            "op inhoud -- vensters van byte-identieke bestanden, niet regelafstand, "
            "die keert om zodra de bestandsverzamelingen verschillen -- of zet "
            "`niet_mergen = \"<reden>\"` zodat het gat een besluit is.",
            file=sys.stderr,
        )
        return 1

    print(f"[forkpunt] OK: {len(forks)} fork(s), elk met een forkpunt of een "
          f"reden om niet te mergen ({len(geblokkeerd)} geblokkeerd).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
