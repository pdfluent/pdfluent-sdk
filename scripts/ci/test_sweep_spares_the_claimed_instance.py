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


def draai(behoud: str, busy_namen: set[str]) -> tuple[str, list[str]]:
    spec = importlib.util.spec_from_file_location("sweep", HIER / "sweep_idle_instances.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)

    oud = (datetime.datetime.now(datetime.timezone.utc)
           - datetime.timedelta(minutes=90)).isoformat()
    namen = ["gh-runner-geclaimd", "gh-runner-oud", "gh-runner-bezig"]
    gewist: list[str] = []

    def nep_haal(url: str, token: str, methode: str = "GET"):
        if methode == "DELETE":
            gewist.append(url.rsplit("/", 1)[-1])
            return {}
        if "/servers" in url:
            return {"servers": [{"name": n, "id": n, "created": oud} for n in namen]}
        if "/runners" in url:
            return {"runners": [{"name": n, "busy": n in busy_namen, "status": "online"}
                                for n in namen], "total_count": len(namen)}
        return {}

    mod.haal = nep_haal
    os.environ["HCLOUD_TOKEN"] = "stub"
    os.environ["GH_RUNNER_PAT"] = "stub"
    os.environ["GITHUB_REPOSITORY"] = "stub/stub"
    os.environ["SWEEP_BEHOUD"] = behoud
    uit = io.StringIO()
    with redirect_stdout(uit):
        mod.main()
    return uit.getvalue(), gewist


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

    if stuk:
        for r in stuk:
            print(f"FAIL: {r}")
        return 1
    print("ok: the sweep spares what this run claimed and still reaps the rest")
    return 0


if __name__ == "__main__":
    sys.exit(main())
