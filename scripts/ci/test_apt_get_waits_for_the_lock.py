#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""`scripts/ci/apt_get.sh` waits for the dpkg lock, and for nothing else (#448).

Hermetic: `apt-get` and `sudo` are stubs on a built PATH, so nothing here
touches a package manager and the test runs anywhere in under a second.

Four questions, and the third is the one that keeps the wrapper honest:

  1. A lock that comes free  -> the install succeeds, on a later attempt.
  2. A lock that never comes free -> the wrapper gives up and says so, rather
     than waiting until the job times out with no explanation.
  3. A failure that is NOT the lock -> ONE attempt, and the apt message is the
     message. A wrapper that retries everything turns "no such package" into
     ten minutes of grinding and pushes the cause off the top of the log.
  4. The arguments reach apt-get unchanged.

Exit codes:
  0  the wrapper behaves as described
  1  it does not
"""
from __future__ import annotations

import os
import pathlib
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
WRAPPER = REPO / "scripts" / "ci" / "apt_get.sh"

LOCK_OUTPUT = (
    "E: Could not get lock /var/lib/dpkg/lock-frontend. It is held by "
    "process 42140 (unattended-upgr)\\n"
    "E: Unable to acquire the dpkg frontend lock "
    "(/var/lib/dpkg/lock-frontend), is another process using it?"
)

# Fails with apt's own lock wording for the first `$1` attempts, then succeeds.
# The counter is a file, because each attempt is a fresh process.
APT_STUB = """#!/usr/bin/env bash
count_file="$STUB_DIR/attempts"
printf '%s\\n' "$*" >> "$STUB_DIR/argv"
n=$(( $(cat "$count_file") + 1 ))
echo "$n" > "$count_file"
if [ "$n" -le "$STUB_FAILING_ATTEMPTS" ]; then
    printf '%b\\n' "{lock}" >&2
    exit 100
fi
if [ -n "${{STUB_OTHER_FAILURE:-}}" ]; then
    echo "E: Unable to locate package frobnicate" >&2
    exit 100
fi
echo "Setting up mupdf-tools ..."
exit 0
""".format(lock=LOCK_OUTPUT)

SUDO_STUB = """#!/usr/bin/env bash
exec "$@"
"""


def stub_dir(tmp: pathlib.Path) -> pathlib.Path:
    d = tmp / "bin"
    d.mkdir()
    (d / "apt-get").write_text(APT_STUB)
    (d / "sudo").write_text(SUDO_STUB)
    for name in ("apt-get", "sudo"):
        (d / name).chmod(0o755)
    (tmp / "attempts").write_text("0")
    return d


def run(tmp: pathlib.Path, *args: str, **env: str) -> subprocess.CompletedProcess:
    d = tmp / "bin"
    environment = dict(os.environ)
    environment["PATH"] = f"{d}:{environment['PATH']}"
    environment["STUB_DIR"] = str(tmp)
    environment["APT_LOCK_INTERVAL_S"] = "0"
    environment.setdefault("STUB_FAILING_ATTEMPTS", "0")
    environment.update(env)
    return subprocess.run(
        ["bash", str(WRAPPER), *args],
        capture_output=True,
        text=True,
        env=environment,
    )


def attempts(tmp: pathlib.Path) -> int:
    return int((tmp / "attempts").read_text().strip())


def main() -> int:
    failures: list[str] = []

    # 1. The lock is held twice and then comes free.
    with tempfile.TemporaryDirectory() as raw:
        tmp = pathlib.Path(raw)
        stub_dir(tmp)
        r = run(tmp, "install", "-y", "-qq", "mupdf-tools",
                STUB_FAILING_ATTEMPTS="2", APT_LOCK_DEADLINE_S="60")
        if r.returncode != 0:
            failures.append(
                "a lock that comes free must end in a successful install; the "
                f"wrapper exited {r.returncode}:\n{r.stdout}\n{r.stderr}"
            )
        if attempts(tmp) != 3:
            failures.append(
                f"expected three attempts (two locked, one through), got {attempts(tmp)}"
            )
        # 4. Arguments reach apt-get unchanged.
        argv = (tmp / "argv").read_text().splitlines()
        if argv and argv[-1] != "install -y -qq mupdf-tools":
            failures.append(f"apt-get was called with {argv[-1]!r}")

    # 2. The lock is never free: the wrapper gives up, and says which lock.
    with tempfile.TemporaryDirectory() as raw:
        tmp = pathlib.Path(raw)
        stub_dir(tmp)
        r = run(tmp, "update", "-qq",
                STUB_FAILING_ATTEMPTS="99", APT_LOCK_DEADLINE_S="0")
        if r.returncode == 0:
            failures.append("a lock that is never free must not end in a pass")
        if "dpkg lock was still held" not in r.stderr:
            failures.append(
                "giving up must say that it was the lock it waited for; got:\n"
                + r.stderr
            )

    # 3. A failure that is not the lock is answered on the first attempt.
    with tempfile.TemporaryDirectory() as raw:
        tmp = pathlib.Path(raw)
        stub_dir(tmp)
        r = run(tmp, "install", "-y", "frobnicate",
                STUB_FAILING_ATTEMPTS="0", STUB_OTHER_FAILURE="1",
                APT_LOCK_DEADLINE_S="60")
        if r.returncode == 0:
            failures.append("a package that does not exist must fail the step")
        if attempts(tmp) != 1:
            failures.append(
                "a failure that is not the lock must not be retried; apt-get ran "
                f"{attempts(tmp)} time(s). Retrying everything is how a real "
                "failure becomes a timeout with its cause scrolled away."
            )
        if "Unable to locate package frobnicate" not in r.stderr:
            failures.append("apt's own message must survive; got:\n" + r.stderr)

    if failures:
        print("[apt-wrapper] FAIL", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print("[apt-wrapper] OK: waits for the lock, gives up loudly, retries nothing else")
    return 0


if __name__ == "__main__":
    sys.exit(main())
