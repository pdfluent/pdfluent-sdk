#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
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
import pathlib
import shutil
import subprocess
import sys
import urllib.error
import urllib.request

# THE WINDOW IN WHICH A RUNNER IS DELETED, and it repeats every hour (#517).
#
# This used to be "older than 50 minutes", counted from the create. That is the
# right rule for the first hour and the wrong one for every hour after it: a
# runner reused into a second hour is past 50 minutes for the whole of it, so
# the first sweep after minute 60 deletes it -- at minute 1 of an hour Hetzner
# has already billed, throwing away fifty-nine minutes of capacity that is paid
# for. `gh-runner-27657` on 10-09-2026 was the other half of the same defect: it
# sat idle for 62 minutes because the only thing that runs this sweep is a
# workflow, and no workflow came.
#
# So the rule is the one the gate pool uses (#507): delete only in the last
# minutes of whichever hour the machine is in. The window has to be wide enough
# for the reaper that enforces it to fall inside it -- a ten-minute agent and a
# three-minute window meet four times out of five -- so it opens one sweep
# interval plus the delete margin before the boundary.
VEEG_INTERVAL_MINUTEN = int(os.environ.get("SWEEP_INTERVAL_MINUTES", "10"))
MARGE_MINUTEN = 3
MINUTEN = 50
# A run waiting longer than this is stuck, not pending. Generous on purpose:
# a real queue behind a busy runner clears in minutes, not hours.
VASTGELOPEN_MINUTEN = 90
HCLOUD = "https://api.hetzner.cloud/v1/servers"
VOORVOEGSEL = "gh-runner-"
# The second kind of instance in this project, and it is not a runner: a gate
# run buys one, ships a tree to it over ssh, runs one lane and deletes it in a
# trap (scripts/cos/remote_gate.sh, #474). Nothing registers with GitHub, so
# none of the machinery below -- busy runners, queued workflow runs, stale
# registrations -- can say anything about one. What can end its life is age.
#
# THREE HOURS, and the margin is the whole point. A landing lane runs forty-five
# to seventy minutes and the timeout that bounds it is 150; a sweep that reaped
# at one hour would delete the machine out from under a live gate, which is the
# defect #274 records in the other direction. Past three hours the trap did not
# run -- the Mac died, the terminal was killed -- and nobody is coming back for
# it.
GATE_VOORVOEGSEL = "gate-"
GATE_MINUTEN = 180


def in_het_venster(minuten: float) -> bool:
    """Whether a machine this old is inside the last minutes of its own hour.

    `minuten` is its whole age; what matters is where that lands inside the hour
    it is currently in. Before the window the hour is bought and unspent, and
    the cheapest machine there is is one that is already standing.
    """
    binnen_het_uur = minuten % 60
    return binnen_het_uur >= 60 - (VEEG_INTERVAL_MINUTEN + MARGE_MINUTEN)


def boek_af(server: dict, minuten: float) -> None:
    """Write the closing ledger line for a runner this sweep just deleted.

    A wave instance is bought by a workflow and never entered anywhere, so the
    balance that the gates are held to did not include a single wave hour
    (#517). Booked here, by the process that deleted it, with the hour Hetzner
    will actually bill -- and never fatal: a machine that is gone is gone, and
    a ledger that could not be written is a smaller problem than a sweep that
    stopped halfway through because of it.
    """
    # A FIXTURE MUST NOT SPEND THE OWNER'S MONEY, and this one did (#517).
    #
    # Within an hour of being written, the cases below booked `gh-runner-eind`
    # and `gh-runner-midden` -- names that never existed -- into
    # ~/.local/share/pdfluent/hetzner-ledger.jsonl for EUR 1.33, which put the
    # day over its cap and sent every gate on this Mac into a local fallback.
    # It is the same defect #454 records for remote_gate.sh, one file over.
    #
    # Replacing `haal` is what a fixture does and nothing else does: this module
    # calls both APIs through it, and no caller in this repository substitutes
    # it. So a run in which it has been replaced must name its own ledger, and
    # one that has not is refused here rather than discovered in the balance an
    # hour later.
    boek = os.environ.get(
        "HETZNER_LEDGER_SCRIPT",
        str(pathlib.Path(__file__).resolve().parents[1] / "cos" / "hetzner_ledger.py"))
    if not pathlib.Path(boek).is_file():
        return
    echt = str(pathlib.Path.home() / ".local/share/pdfluent/hetzner-ledger.jsonl")
    if getattr(haal, "__module__", __name__) != __name__ \
       and os.environ.get("HETZNER_LEDGER", echt) == echt:
        print("    NOT booked: this run substitutes `haal`, which only a fixture does, "
              "while still pointing at the operator's ledger. Set HETZNER_LEDGER to a "
              "path of its own.", file=sys.stderr)
        return
    nu = datetime.datetime.now(datetime.timezone.utc)
    gemaakt = (nu - datetime.timedelta(minutes=minuten)).strftime("%Y-%m-%dT%H:%M:%SZ")
    soort = (server.get("server_type") or {}).get("name", "")
    try:
        subprocess.run(
            [sys.executable, boek, "close", str(server.get("id") or ""), server["name"],
             soort, "", gemaakt, "wave", "swept by the hourly reaper (#517)",
             nu.strftime("%Y-%m-%dT%H:%M:%SZ")],
            check=False, capture_output=True, timeout=60)
    except (OSError, subprocess.SubprocessError) as fout:
        print(f"    the hour was not booked ({fout}); "
              "'remote_gate.sh --ledger reconcile' will find it", file=sys.stderr)


def haal(url: str, token: str, methode: str = "GET"):
    req = urllib.request.Request(url, method=methode,
                                 headers={"Authorization": f"Bearer {token}",
                                          "Accept": "application/vnd.github+json"})
    with urllib.request.urlopen(req, timeout=45) as antwoord:
        ruw = antwoord.read()
        return json.loads(ruw) if ruw else {}


def onder_slot() -> bool:
    """Re-exec under the same file lock the provisioning script takes.

    Claiming and reaping both happen on this one desktop, so a plain lock gives
    the mutual exclusion neither API offers: there is no compare-and-swap on
    either side, so every check is otherwise a point in time (#280). Returns
    False when the lock could not be taken, so the caller declines rather than
    proceeding unprotected.
    """
    if os.environ.get("PDFLUENT_LOCK_HELD"):
        return True
    slot = os.environ.get("PDFLUENT_INSTANCE_LOCK", "/var/tmp/pdfluent-instances.lock")
    flock = shutil.which("flock")
    if flock is not None:
        omgeving = dict(os.environ, PDFLUENT_LOCK_HELD="1")
        os.execve(flock, [flock, "--timeout", "300", slot, sys.executable, *sys.argv],
                  omgeving)
    # NO flock, WHICH IS EVERY MAC (#517). Refusing there made this sweep a
    # no-op on the one machine that now runs it every ten minutes -- and a
    # SKIPPED line every ten minutes reads exactly like a sweep with nothing to
    # do. A directory is created atomically or not at all, which is the whole of
    # what the lock is for; the holder's pid goes inside it so a lock left by a
    # killed sweep can be told from one somebody is using.
    #
    # It is worth being clear about what this lock does NOT do. Provisioning
    # runs on the desktop and this can run on the Mac, and a lock is local to
    # one machine, so it never serialised those two against each other. What
    # does is the check further down: while any workflow run is queued or in
    # progress -- and provisioning is always inside one -- no server is deleted.
    map_ = pathlib.Path(slot + ".d")
    try:
        map_.mkdir()
    except FileExistsError:
        pid = 0
        try:
            pid = int((map_ / "pid").read_text().strip())
        except (OSError, ValueError):
            pid = 0
        levend = False
        if pid > 0:
            try:
                os.kill(pid, 0)
                levend = True
            except OSError:
                levend = False
        if levend:
            print(f"[sweep] another sweep (pid {pid}) holds {map_}; nothing was swept")
            return False
        print(f"[sweep] {map_} was left by a sweep that is gone (pid {pid or '?'}); taking it")
        try:
            (map_ / "pid").write_text(f"{os.getpid()}\n")
        except OSError:
            return False
        return True
    except OSError as fout:
        print(f"SKIPPED (not a pass): could not take {map_} ({fout}); nothing was swept.",
              file=sys.stderr)
        return False
    try:
        (map_ / "pid").write_text(f"{os.getpid()}\n")
    except OSError:
        pass
    import atexit
    atexit.register(lambda: (shutil.rmtree(map_, ignore_errors=True), None)[1])
    return True


def veeg_poortinstanties(servers, token: str) -> int:
    """Delete gate instances past GATE_MINUTEN, on age alone.

    Age alone, because there is no second signal: a gate machine is reached over
    ssh from whichever Mac started it and registers with nothing, so "busy" is
    not a question either API here can answer. The margin below is what stands
    in for it.

    Named `gate-<lane>-<sha7>-<pid>` by scripts/cos/remote_gate.sh, and the
    prefix is the contract between the two files --
    test_the_orphan_sweep_reaps_a_gate_instance.py reads both and fails when
    they stop agreeing.
    """
    nu = datetime.datetime.now(datetime.timezone.utc)
    verwijderd = 0
    poorten = [s for s in servers if s["name"].startswith(GATE_VOORVOEGSEL)]
    print(f"[sweep] {len(poorten)} gate instance(s)")
    for s in poorten:
        gemaakt = datetime.datetime.fromisoformat(s["created"].replace("Z", "+00:00"))
        minuten = (nu - gemaakt).total_seconds() / 60
        if minuten <= GATE_MINUTEN:
            print(f"  gate    {s['name']} ({minuten:.0f} min) — inside the margin, "
                  "a lane may still be running on it")
            continue
        print(f"  DELETE  {s['name']} ({minuten:.0f} min) — a gate whose trap never ran")
        try:
            haal(f"{HCLOUD}/{s['id']}", token, "DELETE")
            verwijderd += 1
        except (urllib.error.URLError, OSError) as fout:
            print(f"    could not delete: {fout}", file=sys.stderr)
    return verwijderd


def main() -> int:
    if not onder_slot():
        return 0

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
    # BEFORE the runner reasoning below, and outside it. Every gate on the other
    # side of that reasoning would be spared for the wrong reason: a gate server
    # holds no runner registration, so `bezet` never names it and it would be
    # read as idle; and the queue check that holds servers back exists because a
    # queued workflow can claim a runner, which a gate machine cannot be. Read
    # through that lens a leaked gate would be spared forever.
    verwijderd_poorten = veeg_poortinstanties(servers, hcloud)
    # Default to holding servers back: if we never got far enough to check the
    # queue, we do not know it is safe to delete one.
    alleen_registraties = True

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
            toen = datetime.datetime.now(datetime.timezone.utc)

            def andere(status: str) -> int:
                bladzijde = haal(
                    f"https://api.github.com/repos/{repo}/actions/runs"
                    f"?status={status}&per_page=100", pat)
                runs = bladzijde.get("workflow_runs")
                if runs is None:
                    return bladzijde.get("total_count", 0)
                telt = 0
                for r in runs:
                    if str(r.get("id")) == str(eigen):
                        continue
                    # A run that has waited for hours is not about to claim a
                    # machine: it is waiting for a runner that does not exist
                    # (#281). Counting it holds the sweep off forever, which
                    # turns this safety check into a permanent off switch --
                    # and it reports success every time it declines.
                    # Only a *queued* run can be stuck forever. A run that is
                    # in progress is holding a machine right now, however long
                    # it has been going -- nightly and fuzz legitimately run for
                    # hours, and ageing one out would delete the runner under a
                    # live build.
                    gemaakt = r.get("created_at") if status == "queued" else None
                    if gemaakt:
                        wacht = (toen - datetime.datetime.fromisoformat(
                            gemaakt.replace("Z", "+00:00"))).total_seconds() / 60
                        if wacht > VASTGELOPEN_MINUTEN:
                            print(f"  stale   run {r.get('id')} has waited {wacht:.0f} min; "
                                  "not treating it as live work")
                            continue
                    telt += 1
                return telt
            lopend, wachtend = andere("in_progress"), andere("queued")
        except (urllib.error.URLError, OSError) as fout:
            print(f"SKIPPED (not a pass): could not read the run queue: {fout}",
                  file=sys.stderr)
            return 0
        if lopend or wachtend:
            # Servers are held back, registrations are not. A registration whose
            # server is already gone cannot be claimed by anything, so removing
            # it strands nobody -- and leaving it makes the list fill with
            # offline names until nobody reads it any more. This gate is about
            # not deleting machines, not about doing nothing.
            print(f"[sweep] {lopend} running and {wachtend} queued run(s); an instance "
                  "can be claimed before its job starts, so no server is deleted now")
            alleen_registraties = True
        else:
            alleen_registraties = False
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
        if alleen_registraties:
            continue
        if s["name"] in behoud:
            print(f"  spared  {s['name']} ({minuten:.0f} min) — claimed by this run")
            continue
        if bezet.get(s["name"], False):
            print(f"  busy    {s['name']} ({minuten:.0f} min) — leaving it")
            continue
        if not in_het_venster(minuten):
            print(f"  idle    {s['name']} ({minuten:.0f} min, minute {minuten % 60:.0f} "
                  "of its hour) — that hour is paid for, keep it for reuse")
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
        print(f"  DELETE  {s['name']} ({minuten:.0f} min) — idle at minute "
              f"{minuten % 60:.0f} of its hour, which is about to end")
        try:
            haal(f"{HCLOUD}/{s['id']}", hcloud, "DELETE")
            verwijderd += 1
            boek_af(s, minuten)
            # Take the registration in the same pass. Leaving it for a later run
            # assumes there will be one, and in a quiet period there is not: the
            # push-triggered reap only runs when somebody pushes. Every sweep
            # was leaving exactly one orphan behind, which is how a runner list
            # fills up with offline names.
            if pat and repo:
                rid = next(
                    (r for r in (haal(f"https://api.github.com/repos/{repo}"
                                      "/actions/runners?per_page=100", pat) or {})
                     .get("runners", []) if r["name"] == s["name"]),
                    None,
                )
                if rid is not None:
                    try:
                        haal(f"https://api.github.com/repos/{repo}/actions/runners/"
                             f"{rid['id']}", pat, "DELETE")
                        print(f"          and its registration")
                    except (urllib.error.URLError, OSError) as fout:
                        print(f"          registration left behind: {fout}",
                              file=sys.stderr)
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

    print(f"[sweep] deleted {verwijderd} runner instance(s) and "
          f"{verwijderd_poorten} gate instance(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
