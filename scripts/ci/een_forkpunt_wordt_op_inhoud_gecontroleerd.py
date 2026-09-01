#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Een forkpunt is waar onze code vandaan komt, niet waar iemand dacht dat hij vandaan kwam.

WAAROM DIT NAAST DE ANDERE TWEE BESTAAT

`the_fork_register_is_verifiable.py` controleert of de versie in de Cargo.toml
van de genoemde commit klopt met wat het register claimt. Dat is een echte
controle, en hij is smaller dan hij lijkt: op 01-09-2026 gemeten door het
forkpunt van pdf-syntax van `3bda7cbc3` naar `758948489` te verplaatsen -- de
waarde die op master stond, negentig commits te laat. Groen. Upstream bumpte het
manifest niet tussen die twee, dus beide dragen `version = "0.5.0"`.

Vijf van de zes foute registerregels hadden precies die vorm: fout binnen één
versievenster. De versiecontrole ving er nul van. Wat ze alle zes wél ving was
inhoud -- bestanden die byte voor byte gelijk zijn aan upstream.

DE REGEL DIE HIER WORDT AFGEDWONGEN

Een bestand dat wij nooit hebben aangeraakt is identiek aan upstream over één
aaneengesloten stuk historie. Tel per upstream-revisie hoeveel van onze
bestanden identiek zijn; het forkpunt hoort bij de revisies met het hoogste
aantal te zitten. Bij `3bda7cbc3` zijn dat er 20 en bij `758948489` 18, dus de
foute waarde valt af.

"Bij de maxima", niet "hét maximum": twee opeenvolgende revisies die de crate
niet allebei aanraken hebben dezelfde boom en dus hetzelfde aantal. Eisen dat er
precies één winnaar is, zou een willekeurige van twee gelijke antwoorden fout
rekenen.

WAT DIT KOST, GEMETEN

Geen enkele blob wordt opgehaald: `git ls-tree` geeft de blob-hashes, en die van
ons rekenen we lokaal uit met dezelfde hash. Op de blobless cachekloon draait dit
dus zonder netwerk.

    hayro-ccitt        25 revisies    2s
    hayro-jbig2       136 revisies    7s
    hayro-syntax      515 revisies   28s

Alleen crates waarvan het forkpunt in deze wijziging verandert worden gescoord,
en het register is in zijn hele bestaan twintig keer gewijzigd. Kosten bij een
wijziging die het register niet aanraakt: nul.

Exitcodes:
    0  elk gecontroleerd forkpunt zit bij de maxima
    1  een forkpunt is niet waar de inhoud zegt dat hij is
    3  kan niet controleren (aangekondigd, nooit stil)
"""

from __future__ import annotations

import hashlib
import os
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REGISTER = ROOT / "docs/UPSTREAM_FORKS.toml"

UPSTREAM_URL = "https://github.com/LaurenzV/hayro.git"
CACHE = Path(
    os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache")
) / "pdfluent" / "hayro-register.git"
_EXPLICIT = os.environ.get("HAYRO_CLONE")
CLONE = Path(_EXPLICIT) if _EXPLICIT else CACHE


def git(*args: str, cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["/usr/bin/git", *args], cwd=cwd, capture_output=True, text=True, check=False
    )


def ensure_clone(needed: list[str]) -> str | None:
    """Cache upstream's history, and make sure it reaches the commits we score.

    A valid but stale cache is the dangerous case, not a missing one. `score()`
    only walks what the clone holds, so a register change pointing at a newer
    commit would be measured against an older history -- and an older commit
    already in the cache can then hold the maximum and pass, while the real
    maximum sits in a revision that was never fetched. Green, on the wrong
    evidence. (Codex, #1609.)
    """
    if CLONE.exists() and git("rev-parse", "--git-dir", cwd=CLONE).returncode == 0:
        missing = [
            c for c in needed
            if git("cat-file", "-e", f"{c}^{{commit}}", cwd=CLONE).returncode != 0
        ]
        # HEAD is refreshed too, not only the named commits: the scoring walks
        # `log HEAD`, so a cache whose HEAD predates upstream's would leave the
        # newest revisions out of the comparison entirely.
        out = git("fetch", "--quiet", "--filter=blob:none", "origin",
                  "+refs/heads/*:refs/heads/*", cwd=CLONE)
        if out.returncode != 0 and missing:
            return (f"cache is missing {len(missing)} recorded commit(s) and the refresh "
                    f"failed: {out.stderr.strip()[:160]}")
        return None
    if _EXPLICIT:
        return f"HAYRO_CLONE={CLONE} is not a git repository"
    CLONE.parent.mkdir(parents=True, exist_ok=True)
    out = git("clone", "--bare", "--filter=blob:none", "--quiet", UPSTREAM_URL, str(CLONE))
    if out.returncode != 0:
        return f"could not clone {UPSTREAM_URL}: {out.stderr.strip()[:200]}"
    return None


def blob_id(path: Path) -> str:
    """git's own object id for a file, computed without invoking git."""
    data = path.read_bytes()
    return hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest()


def our_blobs(src: Path) -> dict[str, str]:
    return {str(p.relative_to(src)): blob_id(p) for p in src.rglob("*.rs")}


def identical_at(sha: str, upstream_dir: str, ours: dict[str, str]) -> int | None:
    """How many of our files are byte-identical to upstream at this revision."""
    out = git("ls-tree", "-r", sha, upstream_dir, cwd=CLONE)
    if out.returncode != 0:
        return None
    same = 0
    for line in out.stdout.splitlines():
        meta, _, path = line.partition("\t")
        parts = meta.split()
        if len(parts) < 3 or parts[1] != "blob" or not path.endswith(".rs"):
            continue
        if ours.get(path[len(upstream_dir) + 1:]) == parts[2]:
            same += 1
    return same


def score(upstream_dir: str, ours: dict[str, str]) -> list[tuple[int, str]]:
    """(identical file count, commit) for every revision touching the crate.

    Only revisions that touch it: everything in between has the same tree and
    therefore the same score, so scoring them adds time and no information.
    """
    revs = git("log", "--format=%H", "HEAD", "--", upstream_dir, cwd=CLONE).stdout.split()
    scored = []
    for sha in revs:
        n = identical_at(sha, upstream_dir, ours)
        if n is not None:
            scored.append((n, sha))
    return scored


def register_touched() -> bool:
    """Did this branch change the register at all?

    All-or-nothing on purpose. The first version scored only the entries whose
    `forkpunt` differed from the merge base, which is cheaper and wrong: setting
    an entry BACK to a value master already carries makes it identical to the
    baseline, so the check skipped it -- and master's value is exactly where the
    wrong ones have come from. Measured: moving pdf-syntax to `758948489`, the
    value master carries and 90 commits too late, passed silently.

    So if the register moved, every entry is scored. That is ~140s, and only on
    the changes that touch this file -- twenty in its entire history.
    """
    if os.environ.get("FORKPUNT_CONTROLEER_ALLES"):
        return True
    for ref in ("github/master", "origin/master", "master"):
        base = git("merge-base", "HEAD", ref, cwd=ROOT)
        if base.returncode != 0:
            continue
        diff = git("diff", "--name-only", base.stdout.strip(), "--",
                   "docs/UPSTREAM_FORKS.toml", cwd=ROOT)
        if diff.returncode == 0:
            return bool(diff.stdout.strip())
    # Cannot work out what changed, so check everything rather than nothing.
    return True


def main() -> int:
    if not REGISTER.exists():
        print(f"SKIPPED (not a pass): {REGISTER} is missing", file=sys.stderr)
        return 3
    needed = [f["forkpunt"] for f in tomllib.loads(REGISTER.read_text())["fork"] if f.get("forkpunt")]
    if (why := ensure_clone(needed)) is not None:
        print(f"SKIPPED (not a pass): {why}", file=sys.stderr)
        return 3

    forks = tomllib.loads(REGISTER.read_text())["fork"]
    scoring = register_touched()

    problems: list[str] = []
    checked = 0
    for f in forks:
        crate, point = f["onze_crate"], f.get("forkpunt")
        if not point:
            continue
        if not scoring:
            continue
        upstream_dir = f"{f['upstream']}/src"
        # Only crates that live in this upstream. lopdf and cff-parser have their
        # own repositories, and pdf-render shares none of upstream's filenames,
        # so there is nothing here to compare.
        if not git("cat-file", "-e", f"HEAD:{upstream_dir}", cwd=CLONE).returncode == 0:
            continue
        src = ROOT / "crates" / crate / "src"
        if not src.is_dir():
            continue

        ours = our_blobs(src)
        scored = score(upstream_dir, ours)
        if not scored:
            continue
        best = max(n for n, _ in scored)
        if best == 0:
            # Nothing matches at any revision. That is not "fine, skip it": the
            # entry names a fork point, so something claims to be verifiable and
            # nothing can verify it. The structural guard passes because the
            # field exists, and the version check passes anywhere inside one
            # version window -- so an unverifiable merge base stayed green
            # through all three. (Codex, #1609.)
            #
            # pdf-render is this case and carries `niet_mergen` instead, which is
            # the honest record; an entry reaching here has a fork point it
            # should not have.
            problems.append(
                f"{crate}: not one of {len(ours)} files is byte-identical to upstream at any "
                f"revision, so `forkpunt = {point}` cannot be checked by content. Either it is "
                "wrong, or this crate has diverged past the method's reach -- in which case it "
                "belongs under `niet_mergen`, not under a fork point."
            )
            continue

        checked += 1
        # Scored directly rather than looked up among the revisions that touch
        # the crate: a fork point is a point in history, and it need not be one
        # of the commits that changed this particular crate. Requiring that was
        # the first version of this check, and it rejected two correct entries.
        here = identical_at(point, upstream_dir, ours)
        if here is None:
            problems.append(f"{crate}: fork point {point} does not resolve in {UPSTREAM_URL}")
        elif here < best:
            winners = sorted({sha[:9] for n, sha in scored if n == best})
            problems.append(
                f"{crate}: {point} has {here} of {len(ours)} files byte-identical to upstream, "
                f"but {best} is reachable at {', '.join(winners[:3])}"
                f"{' and others' if len(winners) > 3 else ''}. "
                "A fork point is where the code came from, not a nearby commit."
            )

    if problems:
        print(f"Forkpunt op inhoud: {len(problems)} entry/entries do not match\n", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        print(
            "\nThe version check cannot see this class -- it is why five of the six "
            "wrong entries survived it. Establish the point by content, or record "
            "`niet_mergen` with the reason.",
            file=sys.stderr,
        )
        return 1

    if checked == 0:
        print("[forkpunt-inhoud] OK: the register did not change in this branch, nothing to score")
    else:
        print(f"[forkpunt-inhoud] OK: {checked} fork point(s) sit at the maximum agreement with upstream")
    return 0


if __name__ == "__main__":
    sys.exit(main())
