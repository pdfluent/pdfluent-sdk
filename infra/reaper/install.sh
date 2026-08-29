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

install -m 644 "$(dirname "$0")/pdfluent-reaper.service" /etc/systemd/system/
install -m 644 "$(dirname "$0")/pdfluent-reaper.timer" /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now pdfluent-reaper.timer
systemctl list-timers pdfluent-reaper.timer --no-pager
echo
echo "Installed. Check a run with: journalctl -u pdfluent-reaper.service -n 40"
