#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Een commitboodschap is een publicatie. Deze houdt tegen wat dat niet mag zijn.

Op 26-08-2026 bleek `pdfluent/pdfluent` publiek te staan, met 253 issues erin --
inclusief commerciële afwegingen, partnernamen en interne infrastructuur. De
repository is dichtgezet, maar het onderliggende gat zat niet in de instelling:
er was nergens een moment waarop iemand zich afvroeg of dít naar buiten mocht.

De klant- en partnernamen staan NIET in dit bestand. Ze staan in een lijst
buiten de boom, standaard `~/.config/pdfluent/interne-termen.txt`, te overrulen
met `PDFLUENT_INTERNE_TERMEN`. Formaat: één term per regel, regels die met `#`
beginnen zijn commentaar, elke term wordt als regex-alternatief gebruikt (punten
dus ontsnappen: `Instantly\\.ai`). Aanvullen doe je daar, nooit hier -- een
verbodslijst die haar eigen termen publiceert lekt precies wat zij tegenhoudt.
Ontbreekt de lijst, dan weigert deze controle zichtbaar in plaats van te slagen.

Commitboodschappen dragen hetzelfde risico en ze zijn moeilijker terug te nemen:
een issue sluit je, een commit staat in elke kloon. Deze repository gaat publiek
(LC10), dus elke boodschap die er nu bij komt, publiceert zichzelf later.

WAT WEL EN WAT NIET
Dit is geen woordenlijst, want de meeste treffers zijn techniek. `password` is
een functie van deze SDK. `Adobe` en `iText` zijn feiten over de wereld waar een
PDF-bibliotheek vrijuit over hoort te praten. Een controle die daarop afgaat,
roept zo vaak vals alarm dat hij binnen een week wordt uitgezet.

Wat wel tegengehouden wordt, is wat alleen intern betekenis heeft:

  commercieel   uitspraken over omzet, klantaantallen, marges, prijsstrategie
                -- "there are no customers yet" hoort in een bestuurskamer
  partners      namen van klanten en partners; die staan onder een afspraak
  infrastructuur hostnamen, privé-adressen, keychain-accounts, groepspaden
                -- geen geheimen op zichzelf, wel een plattegrond

WAAROM DIT IN DE HAAK ZIT EN NIET ALLEEN IN CI
Een commit die al gepusht is, is al gepubliceerd. De haak is het laatste moment
waarop terugnemen nog goedkoop is. CI is de vangnetlaag voor wie de haak niet
heeft geïnstalleerd -- dezelfde opzet als no_ai_attribution.py, en om dezelfde
reden: `core.hooksPath` maakt een haak op de verkeerde plek onzichtbaar.
"""
import os
import re
import subprocess
import sys

# Alleen wat intern betekenis heeft. Techniek staat er bewust niet in.
REGELS = [
    (
        "commercieel",
        re.compile(
            r"\b(no customers yet|geen klanten|customer count|klantaantal|"
            r"run ?rate|winstmarge|profit margin|"
            r"revenue (that|which) does not exist|omzetdoel|verdienmodel|"
            r"go-to-market|prijsstrategie|pricing strategy)\b",
            re.I,
        ),
    ),
    (
        # Hoofdlettergevoelig: `arr` is een gewoon woord en een variabelenaam,
        # `ARR` is een omzetbegrip. Zonder dat onderscheid sloeg dit aan op
        # `docs/archive/viewer-rust-phase-2-plan.md`, waar het niets te zoeken had.
        "commercieel",
        re.compile(r"\b(MRR|ARR)\b"),
    ),
    (
        "infrastructuur",
        re.compile(
            # `10.x.x.x` stond hier ook. Dat is eruit: een Windows-SDK-versie
            # als `10.0.22621.0` is niet van een privé-adres te onderscheiden,
            # en die staat in elk buildscript. 192.168 is het bereik dat hier
            # werkelijk gebruikt wordt.
            #
            # `keychain` als los woord stond hier ook, en dat was fout: de
            # macOS Keychain is een API die de editor gewoon gebruikt
            # (`src-tauri/src/lib.rs`, `nativeServices.ts`). Gevoelig is niet
            # het woord maar de accountnaam, en die haal je op met deze twee
            # commando's.
            r"(\b192\.168\.\d+\.\d+\b|"
            r"gitlab\.com/pdfluent-group)",
            re.I,
        ),
    ),
    (
        # Hoofdlettergevoelig, en met de lengte van een echte Windows-hostnaam.
        # Ongevoelig sloeg dit aan op `desktop-app`, `desktop-only` en
        # `desktop-uitleg` -- gewone woorden in een repository over een
        # desktoptoepassing, en precies het soort vals alarm waardoor een
        # controle binnen een week uitstaat.
        "infrastructuur",
        # Minstens één cijfer: Windows verzint namen als DESKTOP-HFR1SL1.
        # Zonder die eis sloeg dit aan op `DESKTOP-NATIVE`, een constante in de
        # editorcode.
        re.compile(r"\bDESKTOP-(?=[A-Z0-9]*[0-9])[A-Z0-9]{5,}\b"),
    ),
]


# --- de lijst die niet in de boom staat --------------------------------------
#
# De klant- en partnernamen stonden hier letterlijk, in een bestand dat publiek
# meegaat. Een verbodslijst die zijn eigen termen publiceert lekt precies wat
# hij moet tegenhouden, en erger dan de commitboodschappen die hij afving: een
# boodschap kun je herschrijven, een gepubliceerd bronbestand staat in elke
# kloon en is niet meer terug te nemen.
#
# Ze uitzonderen van de eigen scan -- wat ik eerst deed -- dicht het lek niet,
# het zet alleen het alarm uit op het ene bestand waar het het meest telt.
PRIVATE_PAD = os.environ.get(
    "PDFLUENT_INTERNE_TERMEN",
    os.path.expanduser("~/.config/pdfluent/interne-termen.txt"),
)


def private_regel():
    """De partnerregel, of None als de lijst ontbreekt.

    GEEN stille terugval op een ingebouwde lijst: dan zou het bestand dat de
    namen uit de boom houdt ze bij afwezigheid weer introduceren. Ontbreekt de
    lijst, dan kan deze regel niet geoordeeld worden, en dat is geen pass.
    """
    try:
        with open(PRIVATE_PAD, encoding="utf-8") as f:
            termen = [r.strip() for r in f if r.strip() and not r.startswith("#")]
    except OSError:
        return None
    if not termen:
        return None
    global _PRIVE_RX
    # `re.I` is not decoration: the terms are names and a commit message spells
    # them however it feels like. It is also the whole reason `::add-mask::`
    # cannot be relied on -- masking is exact -- so removing it would quietly
    # undo both this rule and the argument for redacting its hits.
    _PRIVE_RX = re.compile(r"\b(" + "|".join(termen) + r")\b", re.I)
    return ("partner", _PRIVE_RX)


def alle_regels():
    """REGELS plus de partnerregel; roept SystemExit op als die niet te laden is."""
    pr = private_regel()
    if pr is None:
        raise SystemExit(
            "geen_interne_zaken: SKIPPED (not a pass) -- de lijst met klant- en "
            f"partnernamen ontbreekt op {PRIVATE_PAD}.\nZet PDFLUENT_INTERNE_TERMEN "
            "naar het pad van die lijst. Zonder haar kan de partnerregel niet "
            "geoordeeld worden, en groen zou hier betekenen dat niemand gekeken heeft."
        )
    return REGELS + [pr]


# --- keychain-aanroepen: een aanwijzer, geen lek ------------------------------
#
# `security find-generic-password` / `find-internet-password` stonden in de
# infrastructuurregel, en elke treffer was vals: het geheim staat in de keychain
# en wordt daar OPGEHAALD. Dat is precies het gedrag dat je wilt zien, en het als
# lek melden leert de lezer de melding te negeren.
#
# Wat wel gevoelig kan zijn, is het label ernaast: `-a <account>` / `-s <service>`.
# Een generiek label (`pypi-token`, `HCLOUD_TOKEN`) zegt hooguit welke dienst we
# gebruiken; een e-mailadres, een hostnaam of een klantnaam als label hoort er
# niet te staan. Deze klasse laat de aanroep door en beoordeelt het operand.
KEYCHAIN = re.compile(r"find-(?:generic|internet)-password")
KEYCHAIN_LABEL = re.compile(r"-[as]\s+['\"]?([A-Za-z0-9@._-]+)")
EMAILACHTIG = re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")


def keychain_overtredingen(regel, regels):
    """Treffers voor een keychain-regel: alleen als het LABEL zelf gevoelig is."""
    if not KEYCHAIN.search(regel):
        return []
    uit = []
    for label in KEYCHAIN_LABEL.findall(regel):
        if EMAILACHTIG.search(label):
            uit.append(("keychain-label", label))
            continue
        for naam, rx in regels:
            if rx.search(label):
                uit.append(("keychain-label", label))
                break
    return uit


def overtredingen(tekst):
    uit = []
    for naam, rx in alle_regels():
        for m in rx.finditer(tekst):
            begin = max(0, m.start() - 40)
            uit.append((naam, m.group(0), tekst[begin:m.end() + 30].replace("\n", " ").strip()))
    return uit


def uit_bestand(pad):
    with open(pad, encoding="utf-8", errors="ignore") as f:
        # Commentaarregels van git tellen niet mee.
        return "\n".join(r for r in f if not r.startswith("#"))


def _git_omgeving() -> dict[str, str]:
    """De omgeving voor git-aanroepen, zonder de GIT_*-variabelen.

    Dit script draait in de commit-msg-haak, en juist daar zet git `GIT_DIR` en
    `GIT_WORK_TREE` -- absoluut, en een subprocess dat ze erft werkt op de
    repository van de haak in plaats van op de map waarin je hem wijst.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


# ONDERGRENS: commits in het bereik >= 1 -- een leeg bereik betekent dat de
# verwijzing niet klopt (`origin/master` niet opgehaald, verkeerde tak), en dan
# slaagt deze controle zonder iets gelezen te hebben. Dat is niet te
# onderscheiden van een schone historie.
MIN_COMMITS = 1


def uit_git(bereik):
    n = subprocess.run(
        ["git", "rev-list", "--count", bereik],
        capture_output=True, text=True, check=True, env=_git_omgeving(),
    ).stdout.strip()
    if int(n or 0) < MIN_COMMITS:
        raise SystemExit(
            f"geen_interne_zaken: {bereik} bevat {n} commits. De verwijzing klopt "
            "niet -- een leeg bereik keurt alles goed zonder iets te lezen."
        )
    return subprocess.run(
        ["git", "log", "--format=%s%n%b", bereik],
        capture_output=True, text=True, check=True, env=_git_omgeving(),
    ).stdout


# ONDERGRENS: bestanden in de boom >= 1 -- dezelfde redenering als MIN_COMMITS.
# `git ls-files` in een lege of verkeerde map geeft nul bestanden terug, en een
# scan van nul bestanden is niet te onderscheiden van een schone boom.
MIN_BESTANDEN = 1



# Dit bestand vindt zijn eigen REGELS: `prijsstrategie` en `MRR` staan er
# letterlijk in. Dat is een uitzondering van dezelfde vorm als die welke ik
# hierboven afwijs, en het verschil is materieel: wat hier nog staat is gewone
# handelswoordenschat, die publiceren lekt niets. Het enige dat wel geheim was
# -- de klant- en partnernamen -- staat niet meer in de boom maar in
# PRIVATE_PAD. De uitzondering onderdrukt dus niets dat ertoe doet meer.
EIGEN_BESTANDEN = {"scripts/ci/geen_interne_zaken.py"}


def _is_tekst(pad):
    """Een nulbyte in de eerste 8 KiB betekent binair. Dekt de corpus-PDF's."""
    try:
        with open(pad, "rb") as f:
            return b"\0" not in f.read(8192)
    except OSError:
        return False


def uit_boom():
    """Doorzoekt de inhoud van elk getrackt tekstbestand.

    `uit_git` leest commitBOODSCHAPPEN. Een publieke repository lekt net zo goed
    via de bestanden zelf: een klantnaam in een testfixture staat na publicatie
    in elke kloon, ongeacht hoe schoon de boodschap was.
    """
    # De vraag is niet of de REPOSITORY interne zaken bevat, maar of de
    # GEPUBLICEERDE boom ze bevat. Een klantnaam in een bestand dat
    # PUBLIC_TREE.toml intern verklaart, verlaat het huis niet. Het predicaat
    # komt uit simulate_public_tree, zodat er één antwoord op "gaat dit mee" is.
    import importlib.util
    import tomllib

    _hier = os.path.dirname(os.path.abspath(__file__))
    _spec = importlib.util.spec_from_file_location(
        "simulate_public_tree", os.path.join(_hier, "simulate_public_tree.py"))
    _stp = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_stp)
    manifest = tomllib.loads(_stp.MANIFEST.read_text())

    paden = [
        p for p in subprocess.run(
            ["git", "ls-files", "-z"],
            capture_output=True, text=True, check=True, env=_git_omgeving(),
        ).stdout.split("\0")
        if p and p not in EIGEN_BESTANDEN and _stp.wordt_gepubliceerd(p, manifest)
    ]
    if len(paden) < MIN_BESTANDEN:
        raise SystemExit(
            f"geen_interne_zaken: de boom bevat {len(paden)} bestanden. De aanroep "
            "klopt niet -- een lege boom keurt alles goed zonder iets te lezen."
        )

    # In geforkte crates telt alleen `commercieel` niet mee: `MRR` in
    # hayro-jpeg2000 is een JPEG2000-coderingspas (SPP -> MRR -> C), geen
    # omzetbegrip. De andere regels blijven wel gelden, want een hostnaam of
    # een klantnaam staat niet in andermans code -- die hebben wij er dan in
    # gezet, via een patch op de fork, en dat is juist het geval dat je wilt zien.
    # `is_geforkt` komt uit header_sweep, dat herkomsttabel.UPSTREAM leest --
    # één bron voor wat geforkt is, geen tweede lijst hier.
    _hs_spec = importlib.util.spec_from_file_location(
        "header_sweep", os.path.join(_hier, "header_sweep.py"))
    _hs = importlib.util.module_from_spec(_hs_spec)
    _hs_spec.loader.exec_module(_hs)

    def _in_fork(pad):
        deel = pad.split("/")
        return (len(deel) > 1 and deel[0] == "crates"
                and _hs.is_geforkt(_hs.CRATES / deel[1]))

    _regels = alle_regels()
    fouten, gelezen = [], 0
    for pad in paden:
        if not _is_tekst(pad):
            continue
        gelezen += 1
        geforkt = _in_fork(pad)
        with open(pad, encoding="utf-8", errors="ignore") as f:
            for nr, regel in enumerate(f, 1):
                for naam, label in keychain_overtredingen(regel, _regels):
                    fouten.append((naam, label, f"{pad}:{nr}", regel.strip()[:90]))
                if KEYCHAIN.search(regel):
                    continue
                for naam, rx in _regels:
                    if geforkt and naam == "commercieel":
                        continue
                    for m in rx.finditer(regel):
                        fouten.append((naam, m.group(0), f"{pad}:{nr}", regel.strip()[:90]))
    return fouten, gelezen


# The compiled private pattern, once `private_regel()` has loaded it. Redaction
# keys on THIS, not on a rule name.
_PRIVE_RX = None


def _toonbaar(naam: str, wat: str, context: str) -> tuple[str, str]:
    """What may appear in the log, for one finding.

    The `partner` rule's terms come from the private list, so printing a hit
    literally publishes the very name the list exists to keep out of the tree --
    and a failing run's log is as public as the tree is. `::add-mask::` does not
    cover it: the rule matches case-insensitively and masking is exact, so a
    differently-cased hit reaches the log unredacted. The context is withheld
    for the same reason, since it is the surrounding text of the term.

    Tied to CI rather than applied always: locally the literal term is what
    makes the message useful, and that log is nobody's but the developer's. The
    position and a short digest are enough to find it in a list you already
    hold. (#1660)
    """
    if not os.environ.get("CI"):
        return wat, context
    # Keyed on the CONTENT, not on the rule that happened to report it.
    # `keychain_overtredingen()` matches a label against every rule INCLUDING
    # the private one and then reports it as `keychain-label`, so a redaction
    # that asked `naam == "partner"` printed the term in full down that path.
    # A rule name is a label; what must not be published is the text.
    if _PRIVE_RX is None:
        return wat, context
    if not (_PRIVE_RX.search(wat) or _PRIVE_RX.search(context or "")):
        return wat, context
    # No digest and no length. `sha256(term.lower())[:8]` with the exact length
    # beside it is reversible with a word list in one line -- a redaction that
    # publishes a checkable fingerprint of the secret is a slower way of
    # publishing the secret. The rule and the position are what a developer
    # needs; the term itself is in the list they already hold.
    return ("<a private term matched here>",
            "<context withheld: it contains the term>")


def _meld_boom(fouten, gelezen):
    if not fouten:
        print(f"OK: {gelezen} publiek wordende tekstbestanden bevatten geen interne zaken.")
        return 0
    print(
        f"geen_interne_zaken: {len(fouten)} plek(ken) in {gelezen} bestanden die\n"
        "PUBLIEK WORDEN horen daar niet. Anders dan een\n"
        "commitboodschap is dit de inhoud zelf: die staat na publicatie in elke kloon.",
        file=sys.stderr,
    )
    for naam, wat, waar, context in fouten[:20]:
        wat, context = _toonbaar(naam, wat, context)
        print(f"  [{naam}] {wat}  --  {waar}: {context}", file=sys.stderr)
    if len(fouten) > 20:
        print(f"  ... en nog {len(fouten) - 20}", file=sys.stderr)
    return 1


def main(argv):
    if "--boom" in argv:
        return _meld_boom(*uit_boom())
    if len(argv) > 1 and not argv[1].startswith("-"):
        tekst, waar = uit_bestand(argv[1]), "deze boodschap"
    else:
        bereik = argv[2] if len(argv) > 2 else "origin/master..HEAD"
        tekst, waar = uit_git(bereik), bereik

    fouten = overtredingen(tekst)
    if not fouten:
        print(f"OK: {waar} bevat geen interne zaken.")
        return 0

    print(
        f"geen_interne_zaken: {len(fouten)} plek(ken) in {waar} horen niet in een\n"
        "commitboodschap. Deze repository gaat publiek; een commit staat daarna in\n"
        "elke kloon en is niet meer terug te nemen.",
        file=sys.stderr,
    )
    for naam, wat, context in fouten[:12]:
        wat, context = _toonbaar(naam, wat, context)
        print(f"  [{naam}] {wat}  --  …{context}…", file=sys.stderr)
    print(
        "\nHerschrijf de boodschap. Wat er technisch gebeurde mag er staan; waarom\n"
        "het commercieel uitkwam, voor welke klant, en op welke machine niet.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
