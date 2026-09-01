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


def de_fetch_gaat_naar_de_cache(clone) -> str | None:
    """Refuse to fetch unless the target really is the cache.

    A second lock, deliberately not resting on the environment being clean. The
    refspec below force-updates every branch, so pointing it at the wrong
    repository destroys unpushed work; `schone_omgeving()` stops that happening,
    and this stops it happening if someone adds a call and forgets.

    `--git-dir` is asked of git itself rather than assumed from `cwd`, because
    the whole failure being guarded against is git disagreeing with `cwd`.
    """
    from pathlib import Path as _P

    # `--git-common-dir`, not `--absolute-git-dir`: a linked worktree's git-dir
    # is `<main>/.git/worktrees/<name>` while its common-dir is `<main>/.git`.
    # Comparing git-dirs both rejected a worktree handed over as `HAYRO_CLONE`
    # -- a legitimate checkout, refused before anything was scored -- and let
    # this repository through whenever the guard itself ran from a worktree,
    # which is how the first version of this check passed its own ROOT test.
    # Common-dirs make both cases structural rather than incidental.
    #
    # Asked in the same environment the fetch will use, which means scrubbed.
    #
    # Review suggested reading the *inherited* environment instead, so the probe
    # could catch a caller that forgot to strip. Measured before taking it, and
    # the measurement said no: `git push` from a linked worktree exports
    # `GIT_DIR`, so under our own pre-push hook every target resolves to this
    # repository and the lock refuses the bare cache, a valid `HAYRO_CLONE` and
    # a worktree alike -- five legitimate cases, all wrongly refused.
    #
    # (An earlier measurement here said hooks do not export `GIT_DIR`. That was
    # taken in a plain clone, where it is true, and it is false in a worktree --
    # which is where all our work happens.)
    #
    # The hazard that suggestion aimed at -- a call that forgets `env=` -- is
    # real, and is caught mechanically one layer up by
    # `test_no_test_can_touch_the_real_repo.py`, which requires `env=` on every
    # git invocation in these scripts. That is the right place for it: a lint
    # over call sites, not a probe guessing at how it will be called.
    out = git("rev-parse", "--git-common-dir", cwd=clone)
    if out.returncode != 0:
        return f"cannot tell which repository {clone} is, so the fetch is refused"
    doel = _P(out.stdout.strip())
    if not doel.is_absolute():
        doel = (_P(clone) / doel)
    doel = doel.resolve()

    # (a) Never this repository, whatever the configuration says.
    #
    # Comparing the target against the path it was *asked* to be is not a lock:
    # for any ordinary checkout `--absolute-git-dir` is `<checkout>/.git`, so
    # accepting that shape accepts the very repository the guard exists to
    # protect. The first version of this check passed its own ROOT test only
    # because it ran in a worktree, whose git-dir is
    # `<main>/.git/worktrees/<name>` and therefore happened not to match.
    # Measured in a plain checkout, the repository root was accepted.
    #
    # So the question is not "is the target the path we wanted" but "is the
    # target us", asked of git from the script's own location.
    hier = _P(__file__).resolve().parent
    ons = git("rev-parse", "--git-common-dir", cwd=hier)
    onze = None
    if ons.returncode == 0:
        onze = _P(ons.stdout.strip())
        # git answers relatively when asked from inside the work tree
        if not onze.is_absolute():
            onze = hier / onze
        onze = onze.resolve()
    if onze is not None and onze == doel:
        return (
            f"refusing to fetch: the target is this repository ({doel}). The refspec "
            "force-updates every branch, so this would rewrite local branches."
        )

    # (b) And it must actually be the upstream, not merely somewhere else.
    herkomst = git("config", "--get", "remote.origin.url", cwd=clone)
    url = herkomst.stdout.strip()
    if not url or "hayro" not in url.lower():
        return (
            f"refusing to fetch: {clone} has origin {url or '<none>'}, which is not "
            "the hayro upstream this cache is for."
        )
    return None


def schone_omgeving() -> dict[str, str]:
    """The caller's environment with every GIT_* variable removed.

    When `GIT_DIR` and `GIT_WORK_TREE` are set, git works on the repository
    they name and ignores `cwd=` entirely. (Measured 02-09: our own
    `pre-commit` and `pre-push` hooks do not export them, so the earlier
    wording here was too strong -- but `git rebase`, CI runners and any
    wrapper that exports them do, and one incident already came of it.) For a read-only command
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


def git(*args: str, cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["/usr/bin/git", *args], cwd=cwd, capture_output=True, text=True, env=schone_omgeving(),
        check=False
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
        if (waarom := de_fetch_gaat_naar_de_cache(CLONE)) is not None:
            return waarom
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
