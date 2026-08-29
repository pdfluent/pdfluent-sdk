#!/usr/bin/env bash
# Install the reaper timer on the WSL desktop. Run as root, inside WSL.
#
# The GitHub schedule this replaces fired twice in a day (#279). Since
# instances are reused rather than deleted after each run, that sweep is the
# only thing that ends their life -- so the clock has to be somewhere that
# actually ticks.
set -euo pipefail

if ! command -v flock >/dev/null 2>&1; then
  echo "flock is required: provisioning and reaping must serialise (#280)." >&2
  exit 1
fi
if [ ! -f /etc/pdfluent/reaper.env ]; then
  echo "Create /etc/pdfluent/reaper.env first, holding HCLOUD_TOKEN and" >&2
  echo "GH_RUNNER_PAT. Without them the sweep cannot tell busy from idle" >&2
  echo "and deletes nothing." >&2
  exit 1
fi
chmod 600 /etc/pdfluent/reaper.env

# The checkout this installer was run from, not a path written months ago.
CHECKOUT="$(cd "$(dirname "$0")/../.." && pwd)"
if [ ! -f "${CHECKOUT}/scripts/ci/sweep_idle_instances.py" ]; then
  echo "No sweep script under ${CHECKOUT}; refusing to install a timer that" >&2
  echo "would fail every 15 minutes." >&2
  exit 1
fi
# python, not sed: a checkout path may legally contain & or the | delimiter,
# and sed would then either reinsert the placeholder or fail outright. A path
# is data, and sed has no way to be told that.
CHECKOUT="${CHECKOUT}" python3 - \
  "$(dirname "$0")/pdfluent-reaper.service" \
  /etc/systemd/system/pdfluent-reaper.service <<'SUBST'
import os, sys
bron, doel = sys.argv[1], sys.argv[2]
# % starts a specifier in a unit file, so a path holding one would be
# expanded rather than used. Doubling it is how systemd takes a literal.
# Escaping for the shell was not enough; each syntax needs its own.
pad = os.environ["CHECKOUT"].replace("%", "%%")
tekst = open(bron).read().replace("@CHECKOUT@", pad)
if "@CHECKOUT@" in tekst:
    raise SystemExit("placeholder survived substitution; refusing to install")
open(doel, "w").write(tekst)
SUBST
chmod 644 /etc/systemd/system/pdfluent-reaper.service
echo "Reaper will run from ${CHECKOUT}"
install -m 644 "$(dirname "$0")/pdfluent-reaper.timer" /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now pdfluent-reaper.timer
systemctl list-timers pdfluent-reaper.timer --no-pager
echo
echo "Installed. Check a run with: journalctl -u pdfluent-reaper.service -n 40"
