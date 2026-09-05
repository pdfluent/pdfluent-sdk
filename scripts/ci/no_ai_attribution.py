#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Geen enkele vorm van AI-attributie in wat PDFluent publiceert.

STAANDE REGEL (Jasper, 25-08-2026, herbevestigd 26-08): commitmetadata is wat
Jasper wil dat ze is. Wat mede door een assistent tot stand kwam, gaat niet als
zodanig naar buiten.

Dit bestand bestaat omdat de regel op 26-08 alsnog werd overtreden: 26 van 29
commits in één nacht droegen `Co-Authored-By: Claude Opus 5`. Niet uit
onwil -- de trailer wordt standaard toegevoegd, dus de default werkt tegen de
afspraak in, en één keer niet opletten is genoeg.

DE VORM VERSCHILT PER MODEL EN PER GEREEDSCHAP
    Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
    Co-Authored-By: Claude <noreply@anthropic.com>
    Co-authored-by: Claude Sonnet 4.5 <...>
    🤖 Generated with [Claude Code](https://claude.com/claude-code)
    Generated with Claude
    Assisted-by: ...

Een lijst met exacte namen veroudert bij het volgende model. Daarom matcht dit
op de *vorm*: een attributieregel plus een aanwijzing naar een assistent, in
welke schrijfwijze dan ook.

TWEE LAGEN
    1. `scripts/git-hooks/commit-msg` haalt het weg bij het schrijven.
    2. Deze controle faalt als het er tóch in staat -- een commit van een andere
       machine, een haak die niet geïnstalleerd is, een `--no-verify`.

De haak alleen is niet genoeg: hij zit in `.git/`, dus hij reist niet mee met de
kloon en hij is met één vlag te omzeilen.
"""
import argparse
import os
import re
import subprocess
import sys


def _git_env() -> dict[str, str]:
    """The environment for git calls, with the GIT_* variables stripped.

    Git hooks set `GIT_DIR`, `GIT_INDEX_FILE` and `GIT_WORK_TREE`, and those
    inherit into every subprocess. This script also runs as the `commit-msg`
    hook, so it is guaranteed to run in such a polluted environment -- and
    then a `git log` points at the caller's directory instead of this one.

    On 25-08-2026 a test set `core.bare = true` on the real repository that
    way and left all thirty worktrees dead (#240).
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}

# Attributieregels: een sleutelwoord aan het begin van een regel, gevolgd door
# een dubbele punt. `Co-Authored-By`, `Co-authored-by`, `Assisted-by`, ...
# THE LOAD-BEARING RULE, and it is closed: every `Co-Authored-By:` goes,
# whatever address it names. The standing rule of 25-08-2026 is that the
# trailer is never added -- so there is no such thing as a legitimate one here,
# and no list of assistants to keep current. A name list ages with the market;
# this does not age at all. It is also why a co-author who IS a person still
# has the trailer stripped: the rule is about the trailer, not about who is on
# it, and a person belongs in the message rather than in metadata git adds by
# default.
TRAILER = re.compile(r"^\s*co[\s-]*authored[\s-]*by\s*:", re.I | re.M)

ATTRIBUTIE = re.compile(
    r"^\s*(co[\s-]*authored[\s-]*by|assisted[\s-]*by|generated[\s-]*(with|by)"
    r"|created[\s-]*(with|by)|written[\s-]*(with|by))\s*[:\-]?\s",
    re.I | re.M,
)

# The SECOND layer, and this one is open by construction: prose forms that
# carry no trailer. Every entry here is a name somebody happened to know when
# they wrote it, and the list is out of date the day a new product ships --
# `Co-authored-by: Devin <devin@cognition.ai>` was missed by two independently
# written guards for exactly that reason. The closed rule above is what this
# repository actually leans on; this catches what arrives without a trailer,
# and it should be read as best-effort rather than as a boundary.
ASSISTENT = re.compile(
    r"(claude|anthropic|chatgpt|openai|gpt-[0-9]|copilot|gemini|codeium"
    r"|codex|cursor|devin|cognition\.ai|noreply@anthropic)",
    re.I,
)

# Losse regels die geen `:` dragen maar wel attributie zijn.
LOSSE_VORMEN = re.compile(
    r"(🤖\s*generated|generated with \[?claude|made with claude"
    r"|met (behulp van )?claude gemaakt)",
    re.I,
)


def is_fout(bericht: str) -> list[str]:
    """De regels in dit bericht die AI-attributie dragen."""
    fout = []
    for regel in bericht.splitlines():
        # The closed rule first: no address is inspected, so nothing here can
        # be defeated by a product nobody has heard of yet.
        if TRAILER.match(regel):
            fout.append(regel.strip())
            continue
        if LOSSE_VORMEN.search(regel):
            fout.append(regel.strip())
            continue
        if ATTRIBUTIE.match(regel) and ASSISTENT.search(regel):
            fout.append(regel.strip())
    return fout


def schoon(bericht: str) -> str:
    """Hetzelfde bericht zonder die regels, en zonder dubbele lege regels."""
    houden = [r for r in bericht.splitlines() if not is_fout(r)]
    uit, vorige_leeg = [], False
    for r in houden:
        leeg = not r.strip()
        if leeg and vorige_leeg:
            continue
        uit.append(r)
        vorige_leeg = leeg
    return "\n".join(uit).rstrip() + "\n"


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--fix", metavar="BESTAND", help="haal de regels uit dit bestand")
    p.add_argument("--range", default=None, help="commitbereik, standaard alles sinds master")
    a = p.parse_args()

    if a.fix:
        import pathlib

        pad = pathlib.Path(a.fix)
        origineel = pad.read_text(errors="replace")
        nieuw = schoon(origineel)
        if nieuw != origineel:
            pad.write_text(nieuw)
        return 0

    bereik = a.range
    if not bereik:
        for basis in ("origin/master", "github/master", "master"):
            r = subprocess.run(["git", "merge-base", basis, "HEAD"],
                               env=_git_env(), capture_output=True, text=True)
            if r.returncode == 0:
                bereik = f"{r.stdout.strip()}..HEAD"
                break
    if not bereik:
        print("SKIPPED (not a pass): no master to compare against", file=sys.stderr)
        return 0

    r = subprocess.run(["git", "log", "--format=%H", bereik],
                       env=_git_env(), capture_output=True, text=True)
    if r.returncode != 0:
        print(f"SKIPPED (not a pass): `git log {bereik}` failed", file=sys.stderr)
        return 0
    hashes = [h for h in r.stdout.split() if h]

    besmet = []
    for h in hashes:
        b = subprocess.run(["git", "log", "-1", "--format=%B", h],
                           env=_git_env(), capture_output=True, text=True).stdout
        fout = is_fout(b)
        if fout:
            onderwerp = subprocess.run(["git", "log", "-1", "--format=%s", h],
                                       env=_git_env(), capture_output=True,
                                       text=True).stdout.strip()
            besmet.append((h[:9], onderwerp[:56], fout[0][:60]))

    if besmet:
        print(
            f"{len(besmet)} of {len(hashes)} commits carry AI attribution.\n",
            file=sys.stderr,
        )
        for h, onderwerp, regel in besmet[:20]:
            print(f"  {h}  {onderwerp}\n      {regel}", file=sys.stderr)
        if len(besmet) > 20:
            print(f"  ... and {len(besmet) - 20} more", file=sys.stderr)
        print(
            "\nProject rule (25-08-2026): what PDFluent publishes carries no AI\n"
            "attribution. Rewrite the range:\n"
            "  FILTER_BRANCH_SQUELCH_WARNING=1 git filter-branch -f \\\n"
            "    --msg-filter 'python3 scripts/ci/no_ai_attribution.py --fix /dev/stdin' \\\n"
            f"    {bereik}\n\n"
            "And install the hook so it does not happen again:\n"
            "  bash scripts/git-hooks/install.sh",
            file=sys.stderr,
        )
        return 1

    print(f"OK: {len(hashes)} commits, no AI attribution.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
