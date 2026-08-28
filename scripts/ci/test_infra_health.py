#!/usr/bin/env python3
"""A long-lived server that is running a job must not be reported as a leak.

The health check used to judge a server on age alone. A `workspace` job here
legitimately runs for hours, so the check called every long build a leaked
machine. An alarm that is wrong while the system is healthy is worse than no
alarm: it teaches you to skip the line that will one day be true.
"""
from __future__ import annotations

import datetime
import importlib.util
import io
import pathlib
import sys
from contextlib import redirect_stderr, redirect_stdout

HIER = pathlib.Path(__file__).resolve().parent


def laad():
    spec = importlib.util.spec_from_file_location("infra_health", HIER / "infra_health.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def draai(busy: bool, minuten: int = 120) -> tuple[int, str]:
    """Run main() against one server that is `minuten` old and busy or not."""
    mod = laad()
    gemaakt = datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(minutes=minuten)
    naam = "gh-runner-test"

    mod.hetzner_token = lambda: "stub"
    mod.servers = lambda _t: [
        {"name": naam, "server_type": {"name": "cpx42"}, "created": gemaakt.isoformat()}
    ]

    def nep_gh(pad: str):
        if pad.endswith("/actions/runners"):
            return {"runners": [{"name": naam, "status": "online", "busy": busy,
                                 "labels": [{"name": "hetzner"}]}]}
        if "/actions/runs" in pad:
            return {"workflow_runs": []}
        return {}

    mod.gh = nep_gh
    # Complaints are written to stderr; capturing only stdout would make every
    # assertion below pass no matter what the check decided.
    uit, fout = io.StringIO(), io.StringIO()
    with redirect_stdout(uit), redirect_stderr(fout):
        code = mod.main()
    return code, uit.getvalue() + fout.getvalue()


def main() -> int:
    stuk = []

    # A busy machine is working, however old it is.
    _, tekst = draai(busy=True)
    if "outlived its run" in tekst:
        stuk.append("a server that is running a job was reported as having outlived it")
    if "OUD" in tekst:
        stuk.append("a busy server was marked OUD in the table")

    # An idle machine past the threshold is exactly what the alarm is for. Without
    # this half the check would pass by never complaining at all.
    _, tekst = draai(busy=False)
    if "outlived its run" not in tekst:
        stuk.append("an idle server well past the threshold raised no alarm")

    # And a young idle machine is not yet a leak.
    _, tekst = draai(busy=False, minuten=5)
    if "outlived its run" in tekst:
        stuk.append("a five-minute-old server was reported as a leak")

    if stuk:
        for r in stuk:
            print(f"FAIL: {r}")
        return 1
    print("ok: the age alarm distinguishes a working machine from a leaked one")
    return 0


if __name__ == "__main__":
    sys.exit(main())
