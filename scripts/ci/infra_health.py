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
# More than this waiting means the desktop is the bottleneck.
WACHTRIJ_ALARM = 4
# What a cpx42 costs, so the report can say what an hour of carelessness cost
# rather than counting machines. Hetzner bills by the hour, rounded up, which
# is why a server that lives four minutes still costs a whole one.
EURO_PER_UUR = {"cpx11": 0.0077, "cpx21": 0.0128, "cpx31": 0.0250,
                "cpx41": 0.0489, "cpx42": 0.0489, "cpx51": 0.0989}
STANDAARD_PER_UUR = 0.05
# More than this at once and something is fanning out rather than sharing.
GELIJKTIJDIG_ALARM = 2


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


def main() -> int:
    klachten: list[str] = []
    nu = datetime.datetime.now(datetime.timezone.utc)
    print(f"[infra] {nu:%Y-%m-%d %H:%M} UTC — {REPO}")

    # --- Hetzner
    token = hetzner_token()
    if token is None:
        print("SKIPPED (not a pass): no Hetzner token, so nothing was checked there.",
              file=sys.stderr)
    else:
        try:
            lijst = servers(token)
        except Exception as fout:  # noqa: BLE001 - reporting, not handling
            print(f"SKIPPED (not a pass): Hetzner unreachable: {fout}", file=sys.stderr)
            lijst = None
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
                merk = "OUD" if minuten > OUD_MINUTEN else "ok"
                print(f"          {merk:3} {s['name']:22} {s['server_type']['name']:8} "
                      f"{minuten:5.0f} min")
                if minuten > OUD_MINUTEN:
                    klachten.append(
                        f"{s['name']} has been up {minuten:.0f} minutes; a job does not take "
                        "that long, so it outlived its run and is billing by the hour"
                    )

    # --- GitHub runners
    runners = gh(f"repos/{REPO}/actions/runners")
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

    # Cost per push, which is the number that decides whether speed was worth
    # buying. A cpx42 for a fifteen-minute build is billed as one hour: about
    # five cents. Two of them in parallel to save four minutes is not a
    # trade-off, it is a doubling for a rounding error.
    if runs is not None and token is not None and lijst is not None:
        vandaag = [r for r in runs.get("workflow_runs", [])
                   if r["created_at"][:10] == f"{nu:%Y-%m-%d}"]
        if vandaag:
            print(f"[infra] cost: {len(vandaag)} run(s) today; a cpx42 hour is "
                  f"EUR {EURO_PER_UUR['cpx42']:.3f}, and Hetzner rounds an hour up -- "
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
