#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# fetch_artefact.sh -- the one way release workflows fetch from the GitHub API.
#
#   fetch_artefact.sh api <route> <out>
#       GET $GITHUB_API_URL/repos/$GITHUB_REPOSITORY/<route>; <out> gets the
#       answer only when it parses as JSON.
#   fetch_artefact.sh zip <artefact-id> <name> <digest> <out>
#       Download artefact <artefact-id> (called <name> in messages) as a zip;
#       <out> gets it only when its sha256 equals <digest> ("sha256:<hex>", the
#       `digest` field artifact API v4 returns per artefact) and it opens as a
#       zip. A missing digest is refused: an unverified download is not used.
#
# Why: 15-10 a Mac runner hit `curl exit 56` and ENOTFOUND on a plain
# `curl -fsSL`, and the smoke went red as if the product had failed. Here
# curl retries what it can (`--retry`, which skips DNS and mid-transfer
# resets), this script retries the transient curl exits curl itself does not
# (6 DNS, 7 connect, 18/56 partial or reset transfer, 28 timeout, ...), every
# download lands in a temp file first, and a download that still fails exits
# non-zero with a message naming it an ENVIRONMENT failure, never a product
# one. HTTP errors (curl exit 22: 4xx, or 5xx after curl's own retries) are
# not retried here: a 404 does not heal by asking again.
#
# Env: GH_TOKEN, GITHUB_API_URL, GITHUB_REPOSITORY (required);
#      RUNNER_TEMP (temp dir, falls back to TMPDIR);
#      FETCH_ATTEMPTS (outer attempts, default 3);
#      FETCH_PAUSE (seconds between outer attempts, default 10);
#      FETCH_RETRY_DELAY (curl --retry-delay, default 5).
set -euo pipefail

die_env() { echo "::error::environment: $*" >&2; exit 75; }
die_use() { echo "fetch_artefact.sh: $*" >&2; exit 2; }

PY="${FETCH_PY:-$(command -v python3 || command -v python || true)}"
[[ -n "$PY" ]] || die_env "no python on PATH to verify downloads with"
: "${GH_TOKEN:?fetch_artefact.sh: GH_TOKEN is not set}"
: "${GITHUB_API_URL:?fetch_artefact.sh: GITHUB_API_URL is not set}"
: "${GITHUB_REPOSITORY:?fetch_artefact.sh: GITHUB_REPOSITORY is not set}"

ATTEMPTS="${FETCH_ATTEMPTS:-3}"
PAUSE="${FETCH_PAUSE:-10}"
RETRY_DELAY="${FETCH_RETRY_DELAY:-5}"
TMP_ROOT="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"

# curl exits worth another attempt: network, not answer.
transient() {
  case "$1" in
    5|6|7|16|18|28|35|52|55|56|92) return 0 ;;
    *) return 1 ;;
  esac
}

# fetch <url> <tmp> <max-time> <what> <verify-fn> [extra curl args...]
fetch() {
  local url="$1" tmp="$2" max_time="$3" what="$4" verify="$5"; shift 5
  local attempt=1 rc
  while :; do
    rm -f "$tmp"
    rc=0
    curl -fsSL --retry 4 --retry-delay "$RETRY_DELAY" --retry-connrefused \
      --connect-timeout 20 --max-time "$max_time" \
      -H "Authorization: Bearer $GH_TOKEN" \
      -H "Accept: application/vnd.github+json" \
      "$@" -o "$tmp" "$url" || rc=$?
    if [[ "$rc" == 0 ]]; then
      if "$verify" "$tmp"; then return 0; fi
      # A truncated answer that curl took for whole: worth another try.
      rc="verify"
    elif ! transient "$rc"; then
      rm -f "$tmp"
      die_env "artefact download failed (curl exit $rc, not a transient error, not retried): $what"
    fi
    if (( attempt >= ATTEMPTS )); then
      rm -f "$tmp"
      die_env "artefact download failed after retries ($attempt attempts, last: curl exit $rc): $what"
    fi
    echo "fetch_artefact.sh: $what: attempt $attempt failed (curl exit $rc), retrying in ${PAUSE}s" >&2
    attempt=$((attempt + 1))
    sleep "$PAUSE"
  done
}

is_json() { "$PY" -c 'import json,sys; json.load(open(sys.argv[1], encoding="utf-8"))' "$1" 2>/dev/null; }
is_zip() { "$PY" -c 'import sys,zipfile; z = zipfile.ZipFile(sys.argv[1]); sys.exit(1 if z.testzip() else 0)' "$1" 2>/dev/null; }

case "${1:-}" in
  api)
    [[ $# == 3 ]] || die_use "usage: api <route> <out>"
    route="$2"; out="$3"
    tmp="$(mktemp "$TMP_ROOT/fetch-api.XXXXXX")"
    fetch "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/$route" "$tmp" 60 "API $route" is_json
    mv -f "$tmp" "$out"
    ;;
  zip)
    [[ $# == 5 ]] || die_use "usage: zip <artefact-id> <name> <digest> <out>"
    aid="$2"; name="$3"; digest="$4"; out="$5"
    want="${digest#sha256:}"
    if [[ -z "$digest" || "$digest" == "None" || "$digest" == "null" || "$want" == "$digest" ]]; then
      echo "::error::artefact $name (id $aid) carries no sha256 digest in the API answer: refusing an unverified download" >&2
      exit 1
    fi
    tmp="$(mktemp "$TMP_ROOT/fetch-zip.XXXXXX")"
    fetch "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/actions/artifacts/$aid/zip" "$tmp" 900 "$name" is_zip -L
    got="$("$PY" -c 'import hashlib,sys; h = hashlib.sha256(); f = open(sys.argv[1], "rb"); [h.update(b) for b in iter(lambda: f.read(1 << 20), b"")]; print(h.hexdigest())' "$tmp" | tr -d '\r')"
    if [[ "$got" != "$(printf '%s' "$want" | tr '[:upper:]' '[:lower:]')" ]]; then
      rm -f "$tmp"
      echo "::error::artefact $name (id $aid): digest mismatch, refusing it: API says sha256:$want, download is sha256:$got" >&2
      exit 1
    fi
    mv -f "$tmp" "$out"
    echo "fetch_artefact.sh: $name verified (sha256:$got)"
    ;;
  *)
    die_use "usage: fetch_artefact.sh api <route> <out> | zip <artefact-id> <name> <digest> <out>"
    ;;
esac
