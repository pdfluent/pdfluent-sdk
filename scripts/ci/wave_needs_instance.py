#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Decide whether a CI wave needs a machine before it can provision one.

An automatic wave tests a revision once, including a failed test verdict.
Repeating an unchanged failure requires workflow_dispatch. A skipped wave is
not evidence of testing: its workspace job must have actually run. API errors
fail the provisioning job instead of authorizing a purchase on missing data.
"""
from __future__ import annotations

import json
import os
import pathlib
import sys
import urllib.parse
import urllib.request


class GitHub:
    def __init__(self, repository: str, token: str):
        self.base = f"https://api.github.com/repos/{repository}/actions"
        self.token = token

    def items(self, path: str, key: str, **filters):
        # The run search API exposes at most 1,000 matching results. Refuse
        # an incomplete answer rather than interpreting it as no prior run.
        for page in range(1, 11):
            query = urllib.parse.urlencode(dict(filters, per_page=100, page=page))
            request = urllib.request.Request(
                f"{self.base}/{path}?{query}",
                headers={"Authorization": f"Bearer {self.token}",
                         "Accept": "application/vnd.github+json"},
            )
            with urllib.request.urlopen(request, timeout=30) as response:
                data = json.load(response)
            rows = data[key]
            if not isinstance(rows, list):
                raise ValueError(f"Invalid {key} response")
            yield from rows
            if len(rows) < 100:
                return
        raise ValueError("Run history exceeds the safe pagination limit")


def skip_reason(api: GitHub, run_id: int, sha: str, event: str) -> str | None:
    workflow = "workflows/ci-ephemeral.yml/runs"
    # Native concurrency owns the pending slot. An older queued run must not
    # suppress the newest push while GitHub is cancelling that pending run.
    statuses = ("in_progress",) if event == "push" else ("in_progress", "queued", "waiting", "pending")
    for status in statuses:
        for run in api.items(workflow, "workflow_runs", status=status):
            if int(run["id"]) < run_id:
                return f"Earlier wave {run['id']} is still {status}."

    if event == "workflow_dispatch":
        return None
    if event not in ("push", "schedule"):
        raise ValueError(f"Unsupported wave event: {event}")

    for run in api.items(workflow, "workflow_runs", head_sha=sha, status="completed"):
        if int(run["id"]) >= run_id or run["head_sha"] != sha:
            continue
        if run["conclusion"] not in ("success", "failure", "timed_out"):
            continue
        for job in api.items(f"runs/{run['id']}/jobs", "jobs"):
            if (job["name"] == "workspace" and job["status"] == "completed"
                    and job["conclusion"] in ("success", "failure", "timed_out")
                    and job.get("runner_id")):
                return (f"Revision {sha} already ran in wave {run['id']} "
                        f"({run['conclusion']}). Use workflow_dispatch to retry.")
    return None


def main() -> int:
    try:
        api = GitHub(os.environ["GITHUB_REPOSITORY"], os.environ["GH_TOKEN"])
        reason = skip_reason(api, int(os.environ["GITHUB_RUN_ID"]),
                             os.environ["GITHUB_SHA"], os.environ["GITHUB_EVENT_NAME"])
        with pathlib.Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
            output.write(f"overslaan={'true' if reason else 'false'}\n")
        print(f"::notice::{reason} No instance requested." if reason else
              "This wave needs an instance.")
        return 0
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"::error::Cannot verify whether a wave needs an instance: {error}. "
              "Provisioning refused.", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
