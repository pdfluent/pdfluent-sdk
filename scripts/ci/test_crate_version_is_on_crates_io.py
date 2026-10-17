#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The publish order asks crates.io, not the workspace it is standing in (#358).

`publish_ordered.sh` asked `cargo info <name>@<version>` from the repo root to
decide whether a crate was already published. Inside the workspace that
resolves to the local path member, so all 32 crates answered "yes", `--live`
skipped every one of them, `cargo publish` was never reached, and the run
printed "done" and exited 0. The same call was the post-publish visibility
wait, so an upload that failed was still announced as visible in the index.

Case 5 is that defect: `pdf-invoice` is a workspace member and is not on
crates.io, and the answer has to be "no" while standing in the workspace. It is
red on the old code by construction -- the old code could not give any other
answer for a workspace member.

The cases run against a stub of the crates.io API on localhost, so they are
deterministic and need no network. Case 5 additionally runs from the repo root,
which is the directory that made the old answer wrong.

The last three cases hold the shape of the run itself (#636): dry-run mode
verifies the whole publish set in one multi-package `cargo publish --dry-run`
(a per-crate dry-run resolves the unpublished `=1.1.0` workspace pins from
crates.io and fails), while live mode still publishes one crate at a time in
topological order. The stub cargo records its argv; nothing is uploaded.
"""
from __future__ import annotations

import http.server
import json
import os
import pathlib
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import threading

REPO = pathlib.Path(__file__).resolve().parents[2]
RULE = REPO / "scripts" / "release" / "crate_version_is_on_crates_io.py"
ORDERED = REPO / "scripts" / "release" / "publish_ordered.sh"

PRESENT, ABSENT, UNKNOWN = 0, 1, 2
MINIMUM_CASES = 25  # FLOOR

# What the stub serves, per crate name.
ROUTES: dict[str, tuple[int, str]] = {}


class Stub(http.server.BaseHTTPRequestHandler):
    def do_GET(self) -> None:  # noqa: N802
        name = self.path.rstrip("/").rsplit("/", 1)[-1]
        status, body = ROUTES.get(name, (404, json.dumps(
            {"errors": [{"detail": f"crate `{name}` does not exist"}]})))
        encoded = body.encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def log_message(self, *_args) -> None:  # keep the test output readable
        return


def crate_with(*versions: str) -> str:
    return json.dumps({"crate": {"max_version": versions[-1] if versions else ""},
                       "versions": [{"num": v} for v in versions]})


def sealed(root: pathlib.Path, api: str) -> dict[str, str]:
    """The fixture's environment, with every GIT_* variable removed.

    The pre-push hook exports GIT_DIR and GIT_WORK_TREE, and a fixture that
    inherits them runs its `git status --porcelain` against the real
    repository. `publish_ordered.sh` then finds the engine dirty and refuses
    to run, and the case fails for a reason that has nothing to do with what
    it measures -- green by hand, red in the gate. The shell fixtures here use
    scripts/cos/_a_fixture_inherits_no_git_env.sh for the same reason.
    """
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env["PATH"] = f"{root / 'bin'}:{env.get('PATH', '')}"
    env["PDFLUENT_CRATES_IO_API"] = api
    return env


def main() -> int:
    failures: list[str] = []
    cases = 0

    def case(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:400]}"))
        if not ok:
            failures.append(what)

    ROUTES.update({
        "pdfluent": (200, crate_with("1.0.0-beta.17", "1.0.0-beta.18")),
        "pdf-engine": (200, crate_with("1.0.0")),
        "server-error": (500, "upstream is having a day"),
        "not-json": (200, "<html>a proxy sign-in page</html>"),
        "no-version-list": (200, json.dumps({"crate": {"max_version": "1.0.0"}})),
        "a-list": (200, "[]"),
        "a-null": (200, "null"),
        "entry-not-object": (200, json.dumps({"versions": ["1.0.0"]})),
        "entry-without-num": (200, json.dumps({"versions": [{"yanked": False}]})),
        "num-not-a-string": (200, json.dumps({"versions": [{"num": 1.0}]})),
        "errors-under-200": (200, json.dumps(
            {"errors": [{"detail": "sign in to continue"}],
             "versions": [{"num": "1.0.0"}]})),
    })

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Stub)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    api = f"http://127.0.0.1:{server.server_address[1]}/api/v1/crates"

    def ask(name: str, version: str, cwd: pathlib.Path | None = None) -> int:
        # A bare environment, so no proxy or token of the caller's reaches the
        # rule -- but not so bare that the interpreter cannot start. The Python
        # actions/setup-python installs on the runner links libpython from its
        # own lib directory and finds it through LD_LIBRARY_PATH; without it
        # every call died before the rule ran, and all 22 cases on master read
        # FAIL with nothing after the dash for weeks (#592).
        env = {"PATH": "/usr/bin:/bin", "PDFLUENT_CRATES_IO_API": api}
        for keep in ("LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH", "PYTHONHOME"):
            if keep in os.environ:
                env[keep] = os.environ[keep]
        done = subprocess.run([sys.executable, str(RULE), name, version],
                              capture_output=True, text=True,
                              cwd=str(cwd) if cwd else None, env=env)
        if done.returncode not in (PRESENT, ABSENT, UNKNOWN):
            print(f"    the rule did not answer (exit {done.returncode}): {done.stderr[-300:]}")
        return done.returncode

    try:
        case("a version that is on crates.io is present",
             ask("pdfluent", "1.0.0-beta.18") == PRESENT)

        case("a version that is not among its versions is absent",
             ask("pdfluent", "1.0.0") == ABSENT)

        case("a crate crates.io does not have is absent",
             ask("pdf-invoice", "1.0.0") == ABSENT)

        # THE DEFECT. `pdf-invoice` is a workspace member; the old rule answered
        # "published" for it because it read ./crates/pdf-invoice.
        case("a workspace member that is not published is absent, asked from the repo root",
             ask("pdf-invoice", "1.0.0", cwd=REPO) == ABSENT)

        case("a published crate is still found when asked from the repo root",
             ask("pdf-engine", "1.0.0", cwd=REPO) == PRESENT)

        # "I could not tell" is not "not published".
        case("an HTTP error is unknown, not absent",
             ask("server-error", "1.0.0") == UNKNOWN)

        case("a body that is not JSON is unknown, not absent",
             ask("not-json", "1.0.0") == UNKNOWN)

        case("a payload with no version list is unknown, not absent",
             ask("no-version-list", "1.0.0") == UNKNOWN)

        # Valid JSON that is not an object: the membership test survives a
        # list and `.get` does not, and the AttributeError would exit 1 --
        # which this contract reads as "not on crates.io". (codex, #1915)
        case("a JSON list is unknown, not absent", ask("a-list", "1.0.0") == UNKNOWN)
        case("a JSON null is unknown, not absent", ask("a-null", "1.0.0") == UNKNOWN)

        # Dropping the entries that do not look like versions leaves a shorter
        # list that still answers, and the answer is "absent" -- out of a body
        # this script could not read. (codex, #1915)
        case("a version entry that is not an object is unknown, not absent",
             ask("entry-not-object", "1.0.0") == UNKNOWN)
        case("a version entry without a num is unknown, not absent",
             ask("entry-without-num", "1.0.0") == UNKNOWN)
        case("a num that is not a string is unknown, not absent",
             ask("num-not-a-string", "1.0.0") == UNKNOWN)

        # A 404 is the registry saying so; an error envelope under HTTP 200 is
        # a proxy contradicting itself, here while carrying the version in the
        # same body. (codex, #1915)
        case("an error envelope under HTTP 200 is unknown, not absent",
             ask("errors-under-200", "1.0.0") == UNKNOWN)

        # A crash must not become an answer: Python exits 1 on an uncaught
        # exception and 1 is "not on crates.io", so a name that cannot go in a
        # URL used to read as a crate that is not published.
        case("a name that cannot go in a URL is unknown, not absent",
             ask("pdfluent 1.0.0", "1.0.0") == UNKNOWN)

        case("an empty version is unknown, not absent",
             ask("pdfluent", "  ") == UNKNOWN)

        # The wiring. Both sites in publish_ordered.sh were the same broken
        # call, so both have to be gone -- the skip check AND the visibility
        # wait, where "could not tell" confirming an upload is the worse half.
        #
        # Only what RUNS is read. A guard that greps the raw file counts the
        # comment explaining the defect as the defect, and the fix would be to
        # delete the explanation.
        shell = ORDERED.read_text(encoding="utf-8")
        runs = "\n".join(line for line in shell.splitlines()
                         if not line.lstrip().startswith("#"))
        asked = len(re.findall(r"crate_on_registry |crate_in_cargo_index ", runs))
        # A BARE `cargo info` IS THE DEFECT, not `cargo info` as such. Inside
        # the workspace it answers out of ./crates/<name>; with `--registry` it
        # asks the index, which is the right surface for the post-publish wait.
        # The blanket ban would have forced the wait onto the wrong one.
        bare = [l for l in runs.splitlines()
                if "cargo info" in l and "--registry" not in l]
        case("publish_ordered.sh runs no cargo info without --registry",
             not bare, bare[:2])
        case("both the skip check and the visibility wait ask outside the workspace",
             asked >= 2, f"asked {asked}x in executed lines")
        case("the rule the wiring names is the one in the tree",
             "crate_version_is_on_crates_io.py" in runs and RULE.is_file())

        # AND THERE IS ONE OF IT. A second publish_ordered.sh sat at the
        # repository root -- older, without any of the 1.0.0 repairs -- and
        # publish-crates.yml ran THAT one at three steps. Both files existed, so
        # `a_workflow_runs_no_script_that_is_not_there.py` was green about it:
        # the path was there, it was just the wrong script. The hosted crates.io
        # publish would have skipped every already-published version without
        # comparing its content. (codex, #1917)
        copies = sorted(
            str(q.relative_to(REPO)) for q in REPO.rglob("publish_ordered.sh")
            if "target" not in q.parts and ".worktrees" not in q.parts)
        case("there is exactly one publish_ordered.sh in the tree",
             copies == ["scripts/release/publish_ordered.sh"], str(copies))

        callers = REPO / ".github" / "workflows" / "publish-crates.yml"
        body = callers.read_text(encoding="utf-8") if callers.is_file() else ""
        case("the crates.io workflow runs that one",
             "scripts/release/publish_ordered.sh" in body
             and "./scripts/publish_ordered.sh" not in body,
             body[:300])

        # AND THE SCRIPT IS RUN, not read. `crate_on_registry ...; seen=$?`
        # is a top-level simple command under `set -euo pipefail`, so the
        # normal answer -- 1, "not published" -- ended the run before the
        # audit and before cargo publish. Reading the file cannot see that;
        # only running it can. (codex, #1915)
        with tempfile.TemporaryDirectory() as fixture:
            root = pathlib.Path(fixture)
            (root / "scripts" / "release").mkdir(parents=True)
            (root / "bin").mkdir()
            for rel in ("scripts/release/publish_ordered.sh",
                        "scripts/release/crate_version_is_on_crates_io.py",
                        # The pre-flight the run opens with. The fixture's stub
                        # answers 404 for a path it does not know, which is
                        # "not published yet" -- so both fixture crates pass it
                        # and this case still measures the errexit shape and
                        # not the pre-flight. (#358)
                        "scripts/release/published_version_matches_the_workspace.py",
                        # And the record that pre-flight holds published
                        # versions against: without it the pre-flight cannot
                        # judge and stops the run, which would measure the
                        # sandbox again.
                        "scripts/release/published_crate_contents.json",
                        # The clean-tree rule, which publish_ordered.sh sources
                        # rather than carrying its own copy of since #358.
                        "scripts/release/tree_dirt.sh",
                        # The publish order, which publish_ordered.sh no
                        # longer computes inline but reads from here (#636).
                        "scripts/release/topological_publish_order.py",
                        # And the owner's release rule (#575), which every
                        # publish path calls and exits on. The fixture carries
                        # it for the same reason it carries the two above: the
                        # case measures the errexit shape of the run, and a
                        # run that stops because a required preflight is not in
                        # the sandbox measures the sandbox.
                        "scripts/release/the_structured_extraction_is_at_parity.sh"):
                shutil.copy2(REPO / rel, root / rel)
            # The preflight reads the committed run files and the reference,
            # which the fixture has no copy of. A stub keeps this case about
            # publish_ordered.sh; `test_structured_parity.py` is where the
            # preflight's own verdict is driven.
            parity = root / "scripts/release/the_structured_extraction_is_at_parity.sh"
            parity.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            audit = root / "scripts" / "release" / "prepublish_crate_audit.sh"
            audit.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            # `kind` is what cargo metadata really prints on every
            # dependency (null for a normal one); the order reads it.
            packages = json.dumps({"packages": [
                {"name": "crate-one", "version": "1.0.0", "dependencies": []},
                {"name": "crate-two", "version": "1.0.0",
                 "dependencies": [{"name": "crate-one", "kind": None}]},
            ]})
            cargo = root / "bin" / "cargo"
            cargo.write_text(
                "#!/bin/sh\n"
                "if [ \"$1\" = metadata ]; then\n"
                f"  cat <<'JSON'\n{packages}\nJSON\n"
                "  exit 0\n"
                "fi\n"
                "echo \"$*\" >> \"${CARGO_ARGV_LOG}\"\n"
                "echo \"stub cargo $*\"\n"
                "exit 0\n", encoding="utf-8")
            for f in (audit, cargo, parity):
                f.chmod(f.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
            argv_log = root / "cargo-argv.log"
            argv_env = sealed(root, api)
            argv_env["CARGO_ARGV_LOG"] = str(argv_log)

            def publishes() -> list[str]:
                if not argv_log.is_file():
                    return []
                return [line for line in argv_log.read_text().splitlines()
                        if line.startswith("publish ")]

            def p_flags(argv: str) -> list[str]:
                names, grab = [], False
                for token in argv.split():
                    if grab:
                        names.append(token)
                        grab = False
                    elif token == "-p":
                        grab = True
                return names

            done = subprocess.run(
                ["bash", str(root / "scripts/release/publish_ordered.sh")],
                capture_output=True, text=True, cwd=str(root),
                env=argv_env)
            reached = done.stdout.count("dry-run publish")
            case("an unpublished crate does not end the run (errexit)",
                 done.returncode == 0 and reached == 2,
                 f"rc={done.returncode}, reached {reached} of 2\n"
                 + done.stdout[-500:] + done.stderr[-500:])

            # DRY-RUN VERIFIES THE WHOLE SET IN ONE INVOCATION (#636). A
            # per-crate `cargo publish --dry-run -p` resolves the `=1.1.0`
            # workspace pins from crates.io, where they do not exist before
            # the release, so the preflight failed on every crate with an
            # unpublished workspace dependency. Red on the old script, which
            # issues one dry-run per crate: two `publish` lines, not one.
            dry_publishes = publishes()
            case("dry-run mode issues one multi-package dry-run",
                 len(dry_publishes) == 1, str(dry_publishes))
            single = dry_publishes[0] if len(dry_publishes) == 1 else ""
            case("the single dry-run covers the full ordered set",
                 "--dry-run" in single
                 and p_flags(single) == ["crate-one", "crate-two"],
                 single or str(dry_publishes))

            # THE LIVE BRANCH IS UNTOUCHED (#636): crates.io is append-only,
            # so a release still publishes one crate at a time, each
            # dependency before its dependents. The stub records the argv;
            # nothing is uploaded. Red if live ever collapses into one
            # multi-package publish or reorders the train.
            argv_log.write_text("", encoding="utf-8")
            live = subprocess.run(
                ["bash", str(root / "scripts/release/publish_ordered.sh"),
                 "--live"],
                capture_output=True, text=True, cwd=str(root),
                env=argv_env)
            live_publishes = publishes()
            case("live mode still publishes one crate at a time, in order",
                 live.returncode == 0
                 and live_publishes == ["publish -p crate-one",
                                       "publish -p crate-two"],
                 f"rc={live.returncode}, {live_publishes}\n"
                 + live.stdout[-500:] + live.stderr[-500:])
    finally:
        server.shutdown()

    print(f"\n{cases} case(s), {len(failures)} failure(s)")
    if cases < MINIMUM_CASES:
        print(f"FAIL: {cases} cases is under the floor of {MINIMUM_CASES}", file=sys.stderr)
        return 1
    if failures:
        print("FAIL: " + "; ".join(failures), file=sys.stderr)
        return 1
    print("OK: the publish order asks crates.io")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
