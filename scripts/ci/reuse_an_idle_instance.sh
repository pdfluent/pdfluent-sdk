#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Reuse before buying.
#
# Hetzner bills a started hour in full, so an instance that is already up and
# idle costs nothing for the rest of that hour. And the runner is registered
# without `--ephemeral`, so it takes more than one job -- which means the second
# push of an evening does not need a second machine.
#
# Three pushes five minutes apart used to buy three instances: three billed
# hours for fifteen minutes of work (#274).
#
# Writes `label` and `reused` to $GITHUB_OUTPUT. `gh` is not installed on the
# desktop runner, so this talks to both APIs with curl.

set -uo pipefail

# One machine provisions and one machine reaps: this desktop. So a plain file
# lock is enough to make claiming and deleting mutually exclusive, which the
# GitHub and Hetzner APIs cannot do between them -- neither offers a
# compare-and-swap, so every check either side is a point in time (#280).
#
# Re-exec under flock rather than wrapping the body, so every exit path
# releases it. The timeout is a deadlock guard: a crashed holder must not stop
# provisioning for ever, and 300s is far longer than either side needs.
SLOT="${PDFLUENT_INSTANCE_LOCK:-/var/tmp/pdfluent-instances.lock}"
if [ -z "${PDFLUENT_LOCK_HELD:-}" ]; then
  if command -v flock >/dev/null 2>&1; then
    export PDFLUENT_LOCK_HELD=1
    exec flock --timeout 300 "${SLOT}" "$0" "$@"
  fi
  echo "SKIPPED (not a pass): flock is not installed, so claiming and reaping" \
       "are not serialised on this host." >&2
fi

uit="${GITHUB_OUTPUT:-/dev/stdout}"
: "${HCLOUD_TOKEN:?HCLOUD_TOKEN is required}"
: "${RUNNER_PAT:?RUNNER_PAT is required}"
: "${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"

servers="$(curl -sf -H "Authorization: Bearer ${HCLOUD_TOKEN}" \
  'https://api.hetzner.cloud/v1/servers?per_page=50' \
  | python3 -c 'import json,sys
d = json.load(sys.stdin).get("servers", [])
print(" ".join(s["name"] for s in d if s["name"].startswith("gh-runner-")))' 2>/dev/null)"

if [ -z "${servers}" ]; then
    echo "no instance is up; one will be created"
    echo "reused=false" >> "${uit}"
    exit 0
fi

runners="$(curl -sf -H "Authorization: Bearer ${RUNNER_PAT}" \
  "https://api.github.com/repos/${GITHUB_REPOSITORY}/actions/runners")"
if [ -z "${runners}" ]; then
    echo "SKIPPED (not a pass): could not read the runner list, so no instance" >&2
    echo "  could be shown to be idle. Creating a new one." >&2
    echo "reused=false" >> "${uit}"
    exit 0
fi

for naam in ${servers}; do
    staat="$(printf '%s' "${runners}" | python3 -c 'import json,sys
naam = sys.argv[1]
for r in json.load(sys.stdin).get("runners", []):
    if r["name"] == naam:
        print("idle" if r["status"] == "online" and not r["busy"] else "busy")
        break
else:
    print("gone")' "${naam}" 2>/dev/null)"
    case "${staat}" in
        idle)
            echo "reusing ${naam}: already paid for, and idle"
            echo "label=${naam}" >> "${uit}"
            echo "reused=true" >> "${uit}"
            exit 0
            ;;
        *)
            echo "${naam} is ${staat}"
            ;;
    esac
done

# REGEL (Jasper, 28-08-2026): er draait nooit meer dan één instance tegelijk.
#
# Als er al een machine staat maar hij is bezet, wachten we tot hij vrijkomt --
# we kopen er geen tweede bij. Een tweede machine kost een heel extra uur en
# levert bij een bouw van tien minuten hooguit een paar minuten wandkloktijd op.
#
# Loopt de wachttijd af, dan stopt dit met een fout in plaats van alsnog te
# kopen. Een pijplijn die wacht is zichtbaar; een rekening die verdubbelt niet.
wacht=0
# Long enough to outlast a whole build. The rule is one instance at a time, so
# a second push must wait for the first to finish -- and workspace takes 15 to
# 19 minutes. A 900s wait guaranteed that two pushes close together failed the
# second one, which reads as a broken pipeline rather than a queue.
while [ "${wacht}" -lt 1800 ]; do
    for naam in ${servers}; do
        staat="$(printf '%s' "${runners}" | python3 -c 'import json,sys
naam = sys.argv[1]
for r in json.load(sys.stdin).get("runners", []):
    if r["name"] == naam:
        print("idle" if r["status"] == "online" and not r["busy"] else "busy")
        break
else:
    print("gone")' "${naam}" 2>/dev/null)"
        if [ "${staat}" = "idle" ]; then
            echo "reusing ${naam} after waiting ${wacht}s"
            echo "label=${naam}" >> "${uit}"
            echo "reused=true" >> "${uit}"
            exit 0
        fi
    done
    sleep 30
    wacht=$((wacht + 30))
    runners="$(curl -sf -H "Authorization: Bearer ${RUNNER_PAT}" \
      "https://api.github.com/repos/${GITHUB_REPOSITORY}/actions/runners" || echo '{}')"
    servers="$(curl -sf -H "Authorization: Bearer ${HCLOUD_TOKEN}" \
      'https://api.hetzner.cloud/v1/servers?per_page=50' \
      | python3 -c 'import json,sys
d = json.load(sys.stdin).get("servers", [])
print(" ".join(s["name"] for s in d if s["name"].startswith("gh-runner-")))' 2>/dev/null)"
    if [ -z "${servers}" ]; then
        echo "the instance went away while waiting; creating one"
        echo "reused=false" >> "${uit}"
        exit 0
    fi
    echo "still busy after ${wacht}s"
done

echo "an instance has been busy for 15 minutes and the rule says never more" >&2
echo "than one at a time. Refusing to buy a second: re-run this once the" >&2
echo "other build is done, or raise the wait if builds legitimately take longer." >&2
exit 1
