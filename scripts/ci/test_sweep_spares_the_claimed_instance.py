#!/usr/bin/env python3
"""The sweep must not delete the instance the current run has claimed.

Reaping moved onto the push (#279) because the */30 schedule fired twice in a
day. That puts the sweep in the same job that provisions, so the machine this
run is about to use is present while the sweep decides. Age and busy-ness are
read a moment before the delete; naming the claimed instance removes the
window rather than narrowing it.
"""
from __future__ import annotations

import datetime
import importlib.util
import io
import os
import pathlib
import sys
from contextlib import redirect_stdout

HIER = pathlib.Path(__file__).resolve().parent


def draai(behoud: str, busy_namen: set[str], in_de_lucht: int = 0,
          vastgelopen: int = 0, lang_bezig: bool = False) -> tuple[str, list[str]]:
    spec = importlib.util.spec_from_file_location("sweep", HIER / "sweep_idle_instances.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)

    oud = (datetime.datetime.now(datetime.timezone.utc)
           - datetime.timedelta(minutes=90)).isoformat()
    vast = (datetime.datetime.now(datetime.timezone.utc)
            - datetime.timedelta(hours=9)).isoformat()
    namen = ["gh-runner-geclaimd", "gh-runner-oud", "gh-runner-bezig"]
    gewist: list[str] = []
    registraties_weg: list[str] = []

    def nep_haal(url: str, token: str, methode: str = "GET"):
        if methode == "DELETE":
            # Two different deletes: a Hetzner server, and a GitHub runner
            # registration. Counting them together hid the whole distinction
            # this test exists for.
            if "/actions/runners/" in url:
                registraties_weg.append(url.rsplit("/", 1)[-1])
            else:
                gewist.append(url.rsplit("/", 1)[-1])
            return {}
        if "/servers" in url:
            return {"servers": [{"name": n, "id": n, "created": oud} for n in namen]}
        if "/actions/runs" in url:
            soort = url.split("status=")[-1].split("&")[0] if "status=" in url else ""
            # Our own run is always present; the sweep must look past it.
            vers = datetime.datetime.now(datetime.timezone.utc).isoformat()
            runs = [{"id": "ONS", "created_at": vers}]
            runs += [{"id": f"ander-{i}", "created_at": vers} for i in range(in_de_lucht)]
            if soort == "queued":
                runs += [{"id": f"vast-{i}", "created_at": vast}
                         for i in range(vastgelopen)]
            # A long build is old *and* in progress. It must never be aged out.
            if soort == "in_progress" and lang_bezig:
                runs += [{"id": "lang", "created_at": vast}]
            return {"workflow_runs": runs, "total_count": len(runs)}
        if "/runners" in url:
            # One registration whose server is gone. It cannot be claimed by
            # anything, so the sweep must clean it even while work is in flight.
            regs = [{"name": n, "busy": n in busy_namen, "status": "online", "id": n}
                    for n in namen]
            regs.append({"name": "gh-runner-weg", "busy": False,
                         "status": "offline", "id": "weg"})
            return {"runners": regs, "total_count": len(regs)}
        return {}

    mod.haal = nep_haal
    os.environ["HCLOUD_TOKEN"] = "stub"
    os.environ["GH_RUNNER_PAT"] = "stub"
    os.environ["GITHUB_REPOSITORY"] = "stub/stub"
    os.environ["SWEEP_BEHOUD"] = behoud
    os.environ["GITHUB_RUN_ID"] = "ONS"
    # Already inside the lock: the unit test must not re-exec under flock.
    os.environ["PDFLUENT_LOCK_HELD"] = "1"
    uit = io.StringIO()
    with redirect_stdout(uit):
        mod.main()
    return uit.getvalue(), gewist


def laat_geclaimd() -> list[str]:
    spec = importlib.util.spec_from_file_location("sweep2", HIER / "sweep_idle_instances.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    oud = (datetime.datetime.now(datetime.timezone.utc)
           - datetime.timedelta(minutes=90)).isoformat()
    beurt = {"n": 0}
    gewist: list[str] = []

    def nep_haal(url: str, token: str, methode: str = "GET"):
        if methode == "DELETE":
            # Two different deletes: a Hetzner server, and a GitHub runner
            # registration. Counting them together hid the whole distinction
            # this test exists for.
            if "/actions/runners/" in url:
                registraties_weg.append(url.rsplit("/", 1)[-1])
            else:
                gewist.append(url.rsplit("/", 1)[-1])
            return {}
        if "/servers" in url:
            return {"servers": [{"name": "gh-runner-laat", "id": "gh-runner-laat",
                                 "created": oud}]}
        if "/actions/runs" in url:
            soort = url.split("status=")[-1].split("&")[0] if "status=" in url else ""
            return {"workflow_runs": [{"id": "ONS"}], "total_count": 1}
        if "/runners" in url:
            beurt["n"] += 1
            return {"runners": [{"name": "gh-runner-laat", "busy": beurt["n"] > 1,
                                 "status": "online"}], "total_count": 1}
        return {}

    mod.haal = nep_haal
    os.environ["SWEEP_BEHOUD"] = ""
    os.environ["GITHUB_RUN_ID"] = "ONS"
    # Already inside the lock: the unit test must not re-exec under flock.
    os.environ["PDFLUENT_LOCK_HELD"] = "1"
    with redirect_stdout(io.StringIO()):
        mod.main()
    return gewist


def main() -> int:
    stuk = []
    _, gewist = draai("gh-runner-geclaimd", {"gh-runner-bezig"})

    if "gh-runner-geclaimd" in gewist:
        stuk.append("the instance claimed by this run was deleted")
    if "gh-runner-bezig" in gewist:
        stuk.append("a busy instance was deleted")
    # Without this the check could pass by never deleting anything at all.
    if "gh-runner-oud" not in gewist:
        stuk.append("an idle instance past its hour was not reaped, so the sweep does nothing")

    # And sparing must be driven by the name, not by luck: with nothing claimed,
    # that same instance is fair game.
    _, gewist2 = draai("", {"gh-runner-bezig"})
    if "gh-runner-geclaimd" not in gewist2:
        stuk.append("an unclaimed idle instance survived, so the exclusion is not name-driven")

    # A run in flight can claim an instance before its job starts, so busy-ness
    # does not show it yet. Nothing may be deleted while that is true.
    _, gewist3 = draai("", set(), in_de_lucht=1)
    if gewist3:
        stuk.append(f"instances were deleted while a run was in flight: {gewist3}")

    # Claimed *during* the sweep: idle on the first read, busy by the time we
    # are about to delete. Every check here is a point in time, so the one that
    # matters is the last one before the delete.
    if "gh-runner-laat" in laat_geclaimd():
        stuk.append("an instance claimed between the first read and the delete was deleted")

    # A run stuck in the queue for nine hours is waiting for a runner that does
    # not exist (#281). Treating it as live work holds the sweep off forever
    # and turns the safety check into a permanent off switch.
    _, gewist4 = draai("", set(), vastgelopen=3)
    if "gh-runner-oud" not in gewist4:
        stuk.append("a run stuck in the queue for hours stopped the sweep entirely")

    # Nightly and fuzz legitimately run for hours. Ageing an in-progress run out
    # would delete the runner under a live build -- the staleness rule is only
    # ever about the queue.
    _, gewist5 = draai("", set(), lang_bezig=True)
    if gewist5:
        stuk.append(f"instances were deleted under a long-running build: {gewist5}")

    # Servers are held back while work is in flight; registrations are not.
    # A registration whose server is gone cannot be claimed by anything, so
    # cleaning it strands nobody -- and holding it back lets the runner list
    # fill with offline names until nobody reads it.
    tekst, gewist6 = draai("", set(), in_de_lucht=1)
    if gewist6:
        stuk.append(f"a server was deleted while work was in flight: {gewist6}")
    if "gh-runner-weg" not in tekst:
        stuk.append("an orphaned registration was left alone just because a run "
                    "was in flight; it cannot be claimed, so cleaning it strands nobody")

    if stuk:
        for r in stuk:
            print(f"FAIL: {r}")
        return 1
    print("ok: the sweep spares what this run claimed and still reaps the rest")
    return 0


if __name__ == "__main__":
    sys.exit(main())
