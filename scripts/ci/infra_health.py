#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Is the desktop-plus-Hetzner-plus-GitHub arrangement still behaving?

Three machines have to agree for a build to happen: GitHub schedules it, the
desktop runner picks up the orchestration, and Hetzner supplies the muscle. Each
can fail in a way the other two do not report.

  * a Hetzner server outlives its run and bills by the hour
  * a runner registration outlives its server, and the list fills with the dead
  * a create-runner wedges and holds the one desktop runner, so nothing moves
  * the desktop is busy with the other CI system and everything queues
  * the Actions budget runs out, which disables hosted runners and leaves
    self-hosted ones working -- so half the pipeline passes and half vanishes

Read-only. Prints what it sees and exits non-zero when something needs a person.
Run it from anywhere with `gh` and a Hetzner token; without either it says which
part it could not check rather than reporting health it did not measure.

# NO-FLOOR: this asks a fixed set of questions about live systems. It discovers
# nothing it could quietly stop finding, and every unavailable source is
# announced rather than skipped.
"""

from __future__ import annotations

import datetime
import json
import os
import subprocess
import sys
import urllib.request

REPO = os.environ.get("PDFLUENT_REPO", "jasperdew/xfa-native-rust")
# A server older than this has outlived any plausible job.
OUD_MINUTEN = 45
# Where the workflows provision. A type that is cheaper but absent here is not
# an option, so the sizing advice has to know this.
LOCATIE = os.environ.get("PDFLUENT_HETZNER_LOCATIE", "fsn1")
# More than this waiting means the desktop is the bottleneck.
WACHTRIJ_ALARM = 4
# What a cpx42 costs, so the report can say what an hour of carelessness cost
# rather than counting machines. Hetzner bills by the hour, rounded up, which
# is why a server that lives four minutes still costs a whole one.
# Filled from the Hetzner catalogue at run time. It used to be a literal table
# and it drifted: it carried cpx42 at 0.0489 while the real gross price was
# 0.1114, so every cost line in this report understated by a factor of two.
# A price is a fact about the world, and this file is the wrong place to keep one.
EURO_PER_UUR: dict[str, float] = {}
STANDAARD_PER_UUR = 0.05
# The type the workflows ask for, so the sizing check has something to judge
# even when nothing is running at the moment.
STANDAARD_TYPE = os.environ.get("PDFLUENT_HETZNER_TYPE", "cx53")
# REGEL (Jasper, 28-08-2026): nooit meer dan één tegelijk. Een tweede machine
# kost een heel extra uur voor hooguit een paar minuten wandkloktijd.
GELIJKTIJDIG_ALARM = 1
# Hetzner bills a started hour in full. Two provisionings inside one hour cost
# two hours for work that would have fitted in one, so this is the number that
# says whether the pipeline is being used or churned.
PER_UUR_ALARM = 3


def gh(pad: str):
    r = subprocess.run(["gh", "api", pad], capture_output=True, text=True, check=False)
    if r.returncode != 0:
        return None
    try:
        return json.loads(r.stdout)
    except json.JSONDecodeError:
        return None


def hetzner_token() -> str | None:
    if os.environ.get("HCLOUD_TOKEN"):
        return os.environ["HCLOUD_TOKEN"]
    r = subprocess.run(
        ["security", "find-generic-password", "-s", "HCLOUD_TOKEN", "-w"],
        capture_output=True, text=True, check=False,
    )
    return r.stdout.strip() or None


def servers(token: str):
    req = urllib.request.Request(
        "https://api.hetzner.cloud/v1/servers?per_page=50",
        headers={"Authorization": f"Bearer {token}"},
    )
    with urllib.request.urlopen(req, timeout=30) as antwoord:
        return json.load(antwoord).get("servers", [])


def alle_runners():
    """Every runner registration, across pages.

    The API returns 30 per page. A partial list is indistinguishable from a
    complete one -- it is still a dict, still non-empty -- and a busy runner
    that fell onto page two would be read as absent. That turns the idle gate
    into the false alarm it exists to prevent: a working machine reported as a
    leaked one. Returns None when the list cannot be trusted, so the caller
    says "could not confirm" instead of guessing.
    """
    antwoord = gh(f"repos/{REPO}/actions/runners?per_page=100&page=1")
    if antwoord is None:
        return None
    gevonden = list(antwoord.get("runners", []))
    verwacht = antwoord.get("total_count")
    bladzijde = 1
    while verwacht is not None and len(gevonden) < verwacht:
        bladzijde += 1
        if bladzijde > 20:  # a runaway pager is a bug, not a reason to loop
            return None
        volgende = gh(f"repos/{REPO}/actions/runners?per_page=100&page={bladzijde}")
        if volgende is None:
            return None
        stapel = volgende.get("runners", [])
        if not stapel:
            break
        gevonden.extend(stapel)
    if verwacht is not None and len(gevonden) != verwacht:
        print(f"SKIPPED (not a pass): read {len(gevonden)} of {verwacht} runner "
              "registrations; an incomplete list cannot decide whether a server is idle.",
              file=sys.stderr)
        return None
    return {"runners": gevonden, "total_count": verwacht}


def catalogus(token: str) -> dict[str, dict]:
    """Every current server type with its cores, memory and hourly gross price."""
    req = urllib.request.Request(
        "https://api.hetzner.cloud/v1/server_types?per_page=60",
        headers={"Authorization": f"Bearer {token}"},
    )
    with urllib.request.urlopen(req, timeout=30) as antwoord:
        rauw = json.load(antwoord).get("server_types", [])
    uit = {}
    for t in rauw:
        if t.get("deprecated"):
            continue
        prijzen = [pr for pr in t["prices"] if pr["location"] == LOCATIE] or t["prices"]
        uit[t["name"]] = {
            "cores": t["cores"], "memory": t["memory"],
            "arch": t["architecture"], "uur": float(prijzen[0]["price_hourly"]["gross"]),
            "hier": any(pr["location"] == LOCATIE for pr in t["prices"]),
        }
    return uit


def beter_formaat(huidig: str, cat: dict[str, dict]) -> list[str]:
    """Types that are at least as big as `huidig` and cost less, in our location.

    Same architecture only: an arm type is cheaper per core but would need a
    different toolchain, and that is a decision, not an optimisation.
    """
    nu = cat.get(huidig)
    if nu is None:
        return []
    return sorted(
        (naam for naam, t in cat.items()
         if t["hier"] and t["arch"] == nu["arch"]
         and t["cores"] >= nu["cores"] and t["memory"] >= nu["memory"]
         and t["uur"] < nu["uur"]),
        key=lambda naam: cat[naam]["uur"],
    )


def main() -> int:
    klachten: list[str] = []
    nu = datetime.datetime.now(datetime.timezone.utc)
    print(f"[infra] {nu:%Y-%m-%d %H:%M} UTC — {REPO}")

    # --- Hetzner
    token = hetzner_token()
    # Read the runners first: a long-lived server that is running a job is working,
    # not leaking, and the age alarm must be able to tell those two apart.
    runners = alle_runners()
    bezet = None
    if runners is not None:
        bezet = {r["name"] for r in runners.get("runners", []) if r.get("busy")}

    if token is None:
        print("SKIPPED (not a pass): no Hetzner token, so nothing was checked there.",
              file=sys.stderr)
    else:
        try:
            lijst = servers(token)
        except Exception as fout:  # noqa: BLE001 - reporting, not handling
            print(f"SKIPPED (not a pass): Hetzner unreachable: {fout}", file=sys.stderr)
            lijst = None
        try:
            cat = catalogus(token)
            EURO_PER_UUR.update({naam: t["uur"] for naam, t in cat.items()})
        except Exception as fout:  # noqa: BLE001 - reporting, not handling
            print(f"SKIPPED (not a pass): could not read the price catalogue: {fout}",
                  file=sys.stderr)
            cat = {}

        # Is the machine we buy the right one? Asked every run, because the
        # catalogue changes under us and a type that was sensible in June can
        # be beaten by a cheaper, larger one in August without anyone looking.
        gebruikt = sorted({s["server_type"]["name"] for s in (lijst or [])}) or [STANDAARD_TYPE]
        for soort in gebruikt:
            beter = beter_formaat(soort, cat)
            if not beter:
                continue
            eerste = cat[beter[0]]
            dit = cat[soort]
            klachten.append(
                f"{soort} ({dit['cores']} cores, {dit['memory']:.0f} GB, "
                f"EUR {dit['uur']:.4f}/h) is beaten in our own location by "
                f"{beter[0]} ({eerste['cores']} cores, {eerste['memory']:.0f} GB, "
                f"EUR {eerste['uur']:.4f}/h). More machine for less money is not a "
                "trade-off to weigh; it is a type to change"
            )

        if lijst is not None:
            per_uur = sum(EURO_PER_UUR.get(s["server_type"]["name"], STANDAARD_PER_UUR)
                          for s in lijst)
            print(f"[infra] hetzner: {len(lijst)} server(s), "
                  f"EUR {per_uur:.3f}/hour while they run")
            if len(lijst) > GELIJKTIJDIG_ALARM:
                klachten.append(
                    f"{len(lijst)} servers at once (EUR {per_uur:.2f}/hour). One event "
                    "should start one machine; more than that is a fan-out, and fan-out "
                    "buys a fraction of the wall-clock for a multiple of the bill"
                )
            for s in lijst:
                gemaakt = datetime.datetime.fromisoformat(s["created"].replace("Z", "+00:00"))
                minuten = (nu - gemaakt).total_seconds() / 60
                werkt = bezet is not None and s["name"] in bezet
                merk = "OUD" if minuten > OUD_MINUTEN and not werkt else "ok"
                print(f"          {merk:3} {s['name']:22} {s['server_type']['name']:8} "
                      f"{minuten:5.0f} min")
                if minuten > OUD_MINUTEN and bezet is not None and s["name"] in bezet:
                    # Working, not leaking. Jobs here legitimately run long (the WASM
                    # smoke test alone takes 10-14 minutes), so age alone says nothing.
                    continue
                if minuten > OUD_MINUTEN:
                    onbekend = ("" if bezet is not None else
                                " (could not read the runner list, so this may be a live job)")
                    klachten.append(
                        f"{s['name']} has been up {minuten:.0f} minutes and is running "
                        f"nothing{onbekend}; it outlived its run and is billing by the hour"
                    )

    # --- GitHub runners (already read above, so age can be judged against busy-ness)
    if runners is None:
        print("SKIPPED (not a pass): could not read the runner list.", file=sys.stderr)
    else:
        levend = {s["name"] for s in (lijst or [])} if token else None
        vast = []
        for r in runners.get("runners", []):
            labels = ",".join(x["name"] for x in r.get("labels", []))
            print(f"[infra] runner: {r['name']:22} {r['status']:8} busy={r['busy']} [{labels}]")
            if r["status"] == "offline" and r["name"].startswith("gh-runner-"):
                if levend is None or r["name"] not in levend:
                    vast.append(r["name"])
        if vast:
            klachten.append(
                f"{len(vast)} runner registration(s) with no server: {', '.join(vast)}. "
                "The sweep should remove these; if they persist it is not running"
            )
        if not any(r["status"] == "online" for r in runners.get("runners", [])):
            klachten.append(
                "no runner is online. Nothing can start -- not the orchestration, and so "
                "not the build either"
            )

    # --- de wachtrij
    runs = gh(f"repos/{REPO}/actions/runs?per_page=30")
    if runs is None:
        print("SKIPPED (not a pass): could not read the run list.", file=sys.stderr)
    else:
        wachtend = [r for r in runs.get("workflow_runs", [])
                    if r["status"] in ("queued", "pending", "in_progress")]
        print(f"[infra] queue: {len(wachtend)} run(s) not finished")
        for r in wachtend[:6]:
            gestart = datetime.datetime.fromisoformat(r["created_at"].replace("Z", "+00:00"))
            print(f"          {r['status']:12} {int((nu - gestart).total_seconds()/60):4} min  "
                  f"{r['name'][:36]}")
        if len(wachtend) > WACHTRIJ_ALARM:
            klachten.append(
                f"{len(wachtend)} runs are waiting. With one runner on the desktop that is a "
                "queue, and a queue is what stops people using the pipeline"
            )
        for r in wachtend:
            gestart = datetime.datetime.fromisoformat(r["created_at"].replace("Z", "+00:00"))
            if (nu - gestart).total_seconds() / 60 > 60:
                klachten.append(
                    f"'{r['name']}' has been going {int((nu - gestart).total_seconds()/60)} "
                    "minutes. Orchestration has a ten-minute cap, so this is either a real "
                    "build or something wedged"
                )
                break

    # How often an instance was started in the last hour. Each start is a whole
    # billed hour, so three pushes five minutes apart cost three hours for
    # fifteen minutes of work. That is not a fan-out and no guard will catch it:
    # it is the shape of how someone is using the pipeline.
    if runs is not None:
        uur_geleden = nu - datetime.timedelta(hours=1)
        starts = [r for r in runs.get("workflow_runs", [])
                  if "ephemeral" in r["name"].lower()
                  and datetime.datetime.fromisoformat(
                      r["created_at"].replace("Z", "+00:00")) > uur_geleden]
        if starts:
            kosten_uur = len(starts) * EURO_PER_UUR.get(STANDAARD_TYPE, STANDAARD_PER_UUR)
            print(f"[infra] churn: {len(starts)} instance(s) started in the last hour "
                  f"(EUR {kosten_uur:.2f} in billed hours)")
            if len(starts) > PER_UUR_ALARM:
                klachten.append(
                    f"{len(starts)} instances started within the hour, EUR "
                    f"{kosten_uur:.2f} of billed hours. Hetzner charges a started hour "
                    "in full, so pushing three times in five minutes buys three hours "
                    "for fifteen minutes of work. Batch the pushes"
                )

    # Cost per push, which is the number that decides whether speed was worth
    # buying. A cpx42 for a fifteen-minute build is billed as one hour: about
    # five cents. Two of them in parallel to save four minutes is not a
    # trade-off, it is a doubling for a rounding error.
    if runs is not None and token is not None and lijst is not None:
        vandaag = [r for r in runs.get("workflow_runs", [])
                   if r["created_at"][:10] == f"{nu:%Y-%m-%d}"]
        if vandaag:
            print(f"[infra] cost: {len(vandaag)} run(s) today; a cpx42 hour is "
                  f"EUR {EURO_PER_UUR.get(STANDAARD_TYPE, STANDAARD_PER_UUR):.3f}, and Hetzner rounds an hour up -- "
                  "so a build that takes ten minutes costs the same as one that takes "
                  "fifty, and splitting it across two machines costs double")

    if not klachten:
        print("[infra] OK: nothing needs a person.")
        return 0

    print(file=sys.stderr)
    print(f"[infra] {len(klachten)} thing(s) need a person:", file=sys.stderr)
    for k in klachten:
        print(f"  - {k}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
