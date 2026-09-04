#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Voorcontrole voor elke CI-job die het corpus leest.
#
# WAAROM DIT NIET `test -d` IS
#
# Op 21-08-2026 lag de doorgekoppelde corpusschijf er ruim vijf uur uit. De
# kernel gaf leesfouten (`hv_storvsc ... cmd 0x28 status: scsi 0x2`, 1915 regels)
# terwijl de koppeling gewoon bleef bestaan. De toenmalige controle
#
#     test -d /mnt/storagebox/corpus/govdocs || { echo "corpus not mounted"; exit 2; }
#
# slaagde daardoor: een stat op een gecachete dentry raakt de schijf niet aan.
# `corpus:text-replace-gate` liep vervolgens 68 minuten zonder één regel uitvoer
# door richting zijn timeout van twee uur, en drie processen bleven in D-state
# achter. Niets in de pipeline zag verschil tussen "traag" en "dood".
#
# Een controle die de gegevens niet aanraakt, toetst niets. Deze leest bytes.
#
# WAAROM ER NIET OP `timeout` WORDT VERTROUWD
#
# Een proces dat vastzit op ononderbreekbare I/O (D-state) is niet te doden, ook
# niet met SIGKILL — `timeout 10 head -c 1024 bestand` hangt dan zelf mee. Daarom
# leest een dochterproces en bewaakt de ouder de klok. Het kind blijft eventueel
# hangen tot de schijf terugkomt of de machine herstart, maar de job faalt binnen
# de deadline mét de juiste diagnose in plaats van uren later op een timeout.
#
# Gebruik:
#   scripts/ci/corpus_preflight.sh [--require-mount] [--deadline N] <corpusmap>
#
# Exitcodes (elk een ander gebrek, zodat de logregel voldoende is):
#   0  bruikbaar
#   2  map bestaat niet
#   3  map is leeg / geen pdf's
#   4  bestand gevonden maar levert geen bytes
#   5  lezen loopt vast (schijf reageert niet)
#   6  pad is geen koppelpunt (alleen met --require-mount)
set -uo pipefail

DEADLINE=15
REQUIRE_MOUNT=0
CORPUS=""

while [ $# -gt 0 ]; do
  case "$1" in
    --require-mount) REQUIRE_MOUNT=1; shift ;;
    --deadline)      DEADLINE="$2"; shift 2 ;;
    -*) echo "corpus-preflight: onbekende optie $1" >&2; exit 64 ;;
    *)  CORPUS="$1"; shift ;;
  esac
done

if [ -z "$CORPUS" ]; then
  echo "corpus-preflight: geef een corpusmap op" >&2
  exit 64
fi

fout() { echo "CORPUS-PREFLIGHT MISLUKT: $*" >&2; }

# Voert een commando uit met een harde wandklok-deadline die de ouder bewaakt.
# Geeft 124 bij overschrijding. De uitvoer van het kind komt op stdout terecht.
# Anders dan `timeout` overleeft dit een kind dat in D-state blijft steken.
met_deadline() {
  local secs="$1"; shift
  local uit; uit="$(mktemp)"
  "$@" >"$uit" 2>/dev/null &
  local kind=$!
  local tienden=$(( secs * 10 ))
  local i=0
  while [ "$i" -lt "$tienden" ]; do
    kill -0 "$kind" 2>/dev/null || break
    sleep 0.1
    i=$(( i + 1 ))
  done
  if kill -0 "$kind" 2>/dev/null; then
    kill -9 "$kind" 2>/dev/null || true   # helpt niet bij D-state, maar wel bij de rest
    rm -f "$uit"
    return 124
  fi
  wait "$kind" 2>/dev/null
  local code=$?
  cat "$uit"
  rm -f "$uit"
  return $code
}

# 1. Is het pad er, en desgevraagd ook echt een koppelpunt?
#    `mountpoint` leest de mounttabel, niet de schijf, dus dit kan niet hangen.
if [ "$REQUIRE_MOUNT" -eq 1 ]; then
  koppelpunt="$CORPUS"
  while [ "$koppelpunt" != "/" ] && ! mountpoint -q "$koppelpunt" 2>/dev/null; do
    koppelpunt="$(dirname "$koppelpunt")"
  done
  if [ "$koppelpunt" = "/" ]; then
    fout "$CORPUS ligt op de rootschijf, niet op een aangekoppeld volume."
    exit 6
  fi
fi

# Let op: `if ! cmd` maakt $? binnen het blok altijd 0, dus de code wordt
# apart bewaard voordat er iets anders gebeurt.
met_deadline "$DEADLINE" test -d "$CORPUS"
code=$?
if [ "$code" -ne 0 ]; then
  if [ "$code" -eq 124 ]; then
    fout "zelfs een stat op $CORPUS loopt vast — de schijf reageert niet."
    exit 5
  fi
  fout "$CORPUS bestaat niet."
  exit 2
fi

# 2. Zoek één pdf. `find -print -quit` stopt bij de eerste, dus dit leest niet
#    de hele map van 223.911 bestanden in.
monster="$(met_deadline "$DEADLINE" find "$CORPUS" -maxdepth 1 -name '*.pdf' -print -quit)"
code=$?
if [ "$code" -eq 124 ]; then
  fout "de map $CORPUS doorzoeken loopt vast — de schijf reageert niet."
  exit 5
fi
if [ -z "$monster" ]; then
  fout "geen enkele pdf in $CORPUS — het corpus is leeg of verkeerd gekoppeld."
  exit 3
fi

# 3. Lees echte bytes. Dit is de controle die het verschil maakt: alles hierboven
#    kan uit de dentry-cache komen terwijl de schijf dood is.
bytes="$(met_deadline "$DEADLINE" head -c 5 "$monster")"
code=$?
if [ "$code" -eq 124 ]; then
  fout "lezen uit $monster loopt vast — de schijf reageert niet (D-state I/O)."
  fout "herstel: draai de geplande taak PDFluent-WSL-CorpusDisk op de buildmachine."
  exit 5
fi
if [ -z "$bytes" ]; then
  fout "$monster levert geen bytes op — leesfout op de schijf."
  exit 4
fi
if [ "$bytes" != "%PDF-" ]; then
  fout "$monster begint niet met %PDF- maar met '$bytes' — corpus is beschadigd."
  exit 4
fi

echo "corpus-preflight: $CORPUS leest goed (monster: $(basename "$monster"))"
exit 0
