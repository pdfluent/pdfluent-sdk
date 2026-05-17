#!/usr/bin/env bash
set -euo pipefail
# Local fallback for the `audit` CI lane.
# Mirrors: bash scripts/release/audit-all-packages.sh
bash scripts/release/audit-all-packages.sh
