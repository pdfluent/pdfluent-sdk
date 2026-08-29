#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Delete instances that are idle and past the hour that was paid for them.

Instances are reused rather than deleted after every run: Hetzner bills a
started hour in full, so a machine that is up and idle is free for the rest of
that hour, and the runner is registered without `--ephemeral` so it takes more
than one job. Deleting after each run threw that hour away and made the next
push buy another (#274).

Which means this sweep, not a delete-runner job, is what ends an instance's
life. Two conditions, and both matter:

  idle   a busy runner must survive at any age, or a sweep kills a build
  old    past MINUTEN, because before that the hour is already paid for and
         the machine may still be reused for nothing

It also removes runner registrations whose server is gone. A deleted server
leaves its registration behind, offline and permanent, and a list that fills
with dead entries is a list nobody reads.

# NO-FLOOR: it acts on whatever two live APIs return. An unreachable one is
# announced, never treated as "nothing to do".
"""

from __future__ import annotations

import datetime
import json
import os
import sys
import urllib.error
import urllib.request

MINUTEN = 50
HCLOUD = "https://api.hetzner.cloud/v1/servers"
VOORVOEGSEL = "gh-runner-"


def haal(url: str, token: str, methode: str = "GET"):
    req = urllib.request.Request(url, method=methode,
                                 headers={"Authorization": f"Bearer {token}",
                                          "Accept": "application/vnd.github+json"})
    with urllib.request.urlopen(req, timeout=45) as antwoord:
        ruw = antwoord.read()
        return json.loads(ruw) if ruw else {}


def main() -> int:
    hcloud = os.environ.get("HCLOUD_TOKEN")
    pat = os.environ.get("GH_RUNNER_PAT")
    repo = os.environ.get("GITHUB_REPOSITORY")
    # The instance this very run is about to use. It is normally young enough
    # to survive the age filter, but the filters read state that can change
    # between the read and the delete; naming it removes the window instead of
    # narrowing it.
    behoud = {n for n in os.environ.get("SWEEP_BEHOUD", "").split(",") if n}
    if not hcloud:
        print("SKIPPED (not a pass): HCLOUD_TOKEN is unset; nothing was swept.",
              file=sys.stderr)
        return 0

    try:
        servers = haal(f"{HCLOUD}?per_page=50", hcloud).get("servers", [])
    except (urllib.error.URLError, OSError, json.JSONDecodeError) as fout:
        print(f"SKIPPED (not a pass): Hetzner unreachable: {fout}", file=sys.stderr)
        return 0

    onze = [s for s in servers if s["name"].startswith(VOORVOEGSEL)]

    # A run that is queued or building may claim an instance between the moment
    # this sweep reads "idle" and the moment it deletes. The runner is claimed
    # before its job starts, so busy-ness does not yet show it. While anything
    # is in flight, reap nothing: the machines cost a started hour either way,
    # and the quiet periods -- which is when a leak actually accumulates -- are
    # still swept.
    if pat and repo:
        try:
            # Our own run is in flight by definition -- the reap job is part of
            # it. Counting ourselves would make this gate refuse every time,
            # which is the same self-defeating shape as sweeping inside
            # provisioning: it would look like a working sweep that never
            # deletes.
            eigen = os.environ.get("GITHUB_RUN_ID")
            def andere(status: str) -> int:
                bladzijde = haal(
                    f"https://api.github.com/repos/{repo}/actions/runs"
                    f"?status={status}&per_page=100", pat)
                runs = bladzijde.get("workflow_runs")
                if runs is None:
                    return bladzijde.get("total_count", 0)
                return sum(1 for r in runs if str(r.get("id")) != str(eigen))
            lopend, wachtend = andere("in_progress"), andere("queued")
        except (urllib.error.URLError, OSError) as fout:
            print(f"SKIPPED (not a pass): could not read the run queue: {fout}",
                  file=sys.stderr)
            return 0
        if lopend or wachtend:
            print(f"[sweep] {lopend} running and {wachtend} queued run(s); an instance "
                  "can be claimed before its job starts, so nothing is deleted now")
            return 0
    print(f"[sweep] {len(onze)} instance(s)")

    bezet: dict[str, bool] = {}
    aanwezig: set[str] = set()
    if pat and repo:
        try:
            for r in haal(f"https://api.github.com/repos/{repo}/actions/runners",
                          pat).get("runners", []):
                bezet[r["name"]] = bool(r["busy"])
                aanwezig.add(r["name"])
        except (urllib.error.URLError, OSError, json.JSONDecodeError) as fout:
            print(f"SKIPPED (not a pass): runner list unreadable ({fout}); no server "
                  "will be deleted, because a busy one cannot be told from an idle one.",
                  file=sys.stderr)
            return 0
    else:
        print("SKIPPED (not a pass): no GH_RUNNER_PAT; a busy instance cannot be told "
              "from an idle one, so none is deleted.", file=sys.stderr)
        return 0

    nu = datetime.datetime.now(datetime.timezone.utc)
    verwijderd = 0
    for s in onze:
        gemaakt = datetime.datetime.fromisoformat(s["created"].replace("Z", "+00:00"))
        minuten = (nu - gemaakt).total_seconds() / 60
        if s["name"] in behoud:
            print(f"  spared  {s['name']} ({minuten:.0f} min) — claimed by this run")
            continue
        if bezet.get(s["name"], False):
            print(f"  busy    {s['name']} ({minuten:.0f} min) — leaving it")
            continue
        if minuten <= MINUTEN:
            print(f"  idle    {s['name']} ({minuten:.0f} min) — still inside its paid "
                  "hour, keep for reuse")
            continue
        # Re-read immediately before deleting, not once before the loop. Every
        # check here is a point in time and none of them is a lock: a run can
        # claim this machine between the reading and the delete. Doing it per
        # server shrinks that window to one round trip instead of the whole
        # loop, which is a narrowing, not a fix -- see #280.
        try:
            if andere("in_progress") or andere("queued"):
                print(f"  claimed {s['name']} — a run appeared while sweeping, leaving it")
                continue
            vers = haal(f"https://api.github.com/repos/{repo}/actions/runners?per_page=100", pat)
            if any(r["name"] == s["name"] and r.get("busy")
                   for r in vers.get("runners", [])):
                print(f"  busy    {s['name']} — claimed since the first read, leaving it")
                continue
        except (urllib.error.URLError, OSError) as fout:
            print(f"    could not re-check {s['name']}, so not deleting it: {fout}",
                  file=sys.stderr)
            continue
        print(f"  DELETE  {s['name']} ({minuten:.0f} min) — idle and past its hour")
        try:
            haal(f"{HCLOUD}/{s['id']}", hcloud, "DELETE")
            verwijderd += 1
        except (urllib.error.URLError, OSError) as fout:
            print(f"    could not delete: {fout}", file=sys.stderr)

    levend = {s["name"] for s in onze}
    for naam in sorted(aanwezig):
        if not naam.startswith(VOORVOEGSEL) or naam in levend:
            continue
        rid = next((r for r in haal(f"https://api.github.com/repos/{repo}/actions/runners",
                                    pat).get("runners", []) if r["name"] == naam), None)
        if rid is None:
            continue
        print(f"  STALE   registration {naam} has no server — removing")
        try:
            haal(f"https://api.github.com/repos/{repo}/actions/runners/{rid['id']}",
                 pat, "DELETE")
        except (urllib.error.URLError, OSError) as fout:
            print(f"    could not remove: {fout}", file=sys.stderr)

    print(f"[sweep] deleted {verwijderd} instance(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
