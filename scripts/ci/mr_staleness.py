#!/usr/bin/env python3
"""Open merge requests that have stopped being merge requests.

WHY THIS EXISTS

On 2026-08-20 this project had fourteen open merge requests. Three were 99 days
old and 1175 commits behind master; seven more were 79 days old and 259 behind.
One of them (!11) contained a correct diagnosis of a CI collision that kept
costing pipeline rounds for two and a half months while the fix sat unmerged.

Nothing was wrong with any of that work. What was missing is that nobody was
counting. An open merge request is a question waiting for an answer, and past a
certain age it stops being a question and becomes an archive pretending to be a
plan: rebasing 1175 commits is not an afternoon, it is doing the work again.

So this counts, on a schedule, and says it out loud.

WHAT IT DOES NOT DO

It does not block unrelated work. A branch is not at fault because someone else's
merge request went stale, and a gate that punishes the wrong change gets switched
off. This runs on a schedule and on demand, never as a merge blocker.

Exit codes:
    0  nothing past the hard threshold
    1  at least one merge request past it
    2  could not run (no token, no network) — never treated as "fine"
"""

from __future__ import annotations

import datetime
import json
import os
import subprocess
import sys
import urllib.request

PROJECT = os.environ.get("MR_PROJECT", "pdfluent-group%2FPDFluent-project")
API = f"https://gitlab.com/api/v4/projects/{PROJECT}"

# Warn early, fail late. The warn line is where a merge request still rebases in
# an afternoon; the fail line is where it stops being cheaper to merge than to
# redo.
WARN_DAYS, FAIL_DAYS = 14, 30
WARN_BEHIND, FAIL_BEHIND = 50, 200


def token() -> str | None:
    for var in ("GITLAB_API_TOKEN", "MR_STALENESS_TOKEN"):
        if os.environ.get(var):
            return os.environ[var]
    # A developer machine keeps it in the keychain; CI passes it as a variable.
    try:
        out = subprocess.run(
            ["security", "find-internet-password", "-a", "claude-pdfluent-api", "-w"],
            capture_output=True, text=True, timeout=10)
        return out.stdout.strip() or None
    except Exception:  # noqa: BLE001
        return None


def api(path: str, tok: str) -> list:
    req = urllib.request.Request(f"{API}{path}", headers={"PRIVATE-TOKEN": tok})
    with urllib.request.urlopen(req, timeout=60) as fh:
        return json.loads(fh.read().decode())


def behind(branch: str) -> int | None:
    """Commits on master that this branch does not have."""
    for ref in (f"origin/{branch}", branch):
        r = subprocess.run(["git", "rev-list", "--count", f"{ref}..origin/master"],
                           capture_output=True, text=True)
        if r.returncode == 0 and r.stdout.strip().isdigit():
            return int(r.stdout.strip())
    return None


def main() -> None:
    tok = token()
    if not tok:
        print("[mr_staleness] FATAL: no API token.", file=sys.stderr)
        print("[mr_staleness]   CI: set GITLAB_API_TOKEN (api scope, read is enough).",
              file=sys.stderr)
        print("[mr_staleness]   Local: the keychain entry `claude-pdfluent-api`.",
              file=sys.stderr)
        print("[mr_staleness] Exiting 2, not 0: a check that cannot run has not passed.",
              file=sys.stderr)
        sys.exit(2)

    try:
        mrs = api("/merge_requests?state=opened&per_page=100", tok)
    except Exception as e:  # noqa: BLE001
        print(f"[mr_staleness] FATAL: could not read the merge requests: {e}", file=sys.stderr)
        sys.exit(2)

    subprocess.run(["git", "fetch", "-q", "origin"], capture_output=True)
    now = datetime.datetime.now(datetime.timezone.utc)

    rows = []
    for m in mrs:
        created = datetime.datetime.fromisoformat(m["created_at"].replace("Z", "+00:00"))
        rows.append({
            "iid": m["iid"],
            "days": (now - created).days,
            "behind": behind(m["source_branch"]),
            "branch": m["source_branch"],
            "status": m.get("detailed_merge_status", "?"),
        })
    rows.sort(key=lambda r: -r["days"])

    print(f"[mr_staleness] {len(rows)} open merge request(s)")
    print()
    print(f"  {'MR':>5} {'age':>6} {'behind':>7}  {'state':18} branch")
    print("  " + "-" * 76)
    failing, warning = [], []
    for r in rows:
        b = r["behind"]
        bad = r["days"] >= FAIL_DAYS or (b is not None and b >= FAIL_BEHIND)
        warn = not bad and (r["days"] >= WARN_DAYS or (b is not None and b >= WARN_BEHIND))
        mark = "  <-- stale" if bad else ("  <-- ageing" if warn else "")
        print(f"  !{r['iid']:>4} {r['days']:>5}d {str(b if b is not None else '?'):>7}"
              f"  {r['status'][:18]:18} {r['branch'][:34]}{mark}")
        (failing if bad else warning if warn else []).append(r)

    print()
    if warning:
        print(f"[mr_staleness] {len(warning)} ageing (past {WARN_DAYS} days or "
              f"{WARN_BEHIND} commits behind) — still cheap to rebase, do it now")
    if not failing:
        print("[mr_staleness] nothing past the hard threshold")
        sys.exit(0)

    print(f"[mr_staleness] FAIL: {len(failing)} merge request(s) past "
          f"{FAIL_DAYS} days or {FAIL_BEHIND} commits behind:")
    for r in failing:
        print(f"  - !{r['iid']} ({r['days']}d, {r['behind']} behind) {r['branch']}")
    print()
    print("[mr_staleness] Each needs a decision, and 'leave it open' is not one:")
    print("[mr_staleness]   merge it, rebase it now while that is still an afternoon,")
    print("[mr_staleness]   or close it and say why. The branch survives closing, so")
    print("[mr_staleness]   closing costs nothing but the pretence that it is queued.")
    sys.exit(1)


if __name__ == "__main__":
    main()
