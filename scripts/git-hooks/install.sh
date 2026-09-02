#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# Installeer de haken uit scripts/git-hooks/ in deze kloon.
#
# Haken zitten in `.git/` en reizen niet mee met een kloon, dus dit moet één keer
# per werkkopie gebeuren. Vergeet je het, dan vangt `sanity:no-ai-attribution` het
# alsnog — dat is de reden dat er twee lagen zijn.
set -euo pipefail
WORTEL="$(git rev-parse --show-toplevel)"

# `core.hooksPath` wint van `.git/hooks`. Deze repo zet hem op `.githooks`, en
# een haak in `.git/hooks` wordt dan gewoon genegeerd -- op 26-08 installeerde ik
# hem daar, de test leek te slagen (het script werkte los prima) en de echte
# commit droeg de trailer alsnog. Een haak op de verkeerde plek is niet te
# onderscheiden van geen haak.
HOOKSPATH="$(git config --get core.hooksPath || true)"
if [ -n "$HOOKSPATH" ]; then
    case "$HOOKSPATH" in
        /*) HOOKS="$HOOKSPATH" ;;
        *)  HOOKS="$WORTEL/$HOOKSPATH" ;;
    esac
else
    HOOKS="$(git rev-parse --git-common-dir)/hooks"
fi
# If core.hooksPath points at the tracked `.githooks/` directory, the hooks are
# already carried by the repository and there is nothing to install. Writing
# here would do active harm: `ln -sf` below writes an ABSOLUTE path, and an
# absolute symlink committed into the tree is dangling in every checkout that
# does not sit at that one path -- which is exactly how layer 1 came to be dead
# outside a single worktree (#229). Measured 31-08-2026: running this script in
# a worktree replaced the tracked relative link with an absolute one, staged and
# ready to commit. The installer reintroduced the defect it exists to prevent.
if [ "$HOOKS" = "$WORTEL/.githooks" ]; then
    echo "[git-hooks] core.hooksPath is the tracked .githooks/ -- hooks travel with"
    echo "[git-hooks] the repository, nothing to install."
    for haak in "$WORTEL"/.githooks/*; do
        naam="$(basename "$haak")"
        if [ ! -e "$haak" ]; then
            echo "[git-hooks] BROKEN: $naam does not resolve -- the tracked link is dangling" >&2
            exit 1
        fi
        echo "[git-hooks] $naam OK"
    done
    exit 0
fi

echo "[git-hooks] doel: $HOOKS"
mkdir -p "$HOOKS"
for haak in "$WORTEL"/scripts/git-hooks/*; do
    naam="$(basename "$haak")"
    [ "$naam" = "install.sh" ] && continue
    if [ -e "$HOOKS/$naam" ] && [ ! -L "$HOOKS/$naam" ]; then
        echo "[git-hooks] $naam bestaat al en is geen symlink — met rust gelaten"
        echo "[git-hooks]   voeg met de hand toe: $haak"
        continue
    fi
    ln -sf "$haak" "$HOOKS/$naam"
    echo "[git-hooks] $naam geïnstalleerd"
done
