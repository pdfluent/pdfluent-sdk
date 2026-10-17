#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for scripts/corpus-download-govdocs.sh, with no network and no bucket.

WHAT IS BEING GUARDED

The govdocs subset is the only part of the working corpus that comes back from
public sources; the curated subtrees on the dead disk do not. So "the corpus is
reproducible" rests entirely on this script running on the machine that needs
it -- and measured 07-09-2026 it could not, because it reached the bucket only
through the AWS CLI and the CI runner has no `aws` (#374). The bucket is public.
The signature the CLI adds is not what makes the read work, and a dependency
that is not needed is a dependency that can be missing.

Every case here runs the script with `--dry-run` and a PATH built for the case,
so the whole file is a few seconds and touches nothing outside a temp directory.

  no aws on the machine      the run picks curl and reads the bucket over HTTPS
  aws on the machine         it is still preferred; this is not a replacement
  aws asked for, not there   refused, loudly, instead of downloading nothing
  a fetcher nobody has       refused rather than defaulted
  what a dry run promises    the per-zip figure is the measured one, not four
                             times more documents than a real run would fetch
  every zip unreadable       a short corpus is a failure, not a summary line,
                             and an unreachable source is given up on rather
                             than retried a thousand times
  the suite runner           asking it for govdocs fetches govdocs, instead of
                             printing three steps for a person to follow

The last one is the same rule scripts/corpus-download.sh was given in #276: a
suite that produced no file used to print SKIP and exit 0, and four corpus
gates went months measuring nothing. A run that asks for 5000 documents and
gets 40 states its numbers against the word "govdocs" as if it named a fixed
thing, so it has to fail.
"""

from __future__ import annotations

import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts" / "corpus-download-govdocs.sh"
SUITES = REPO / "scripts" / "corpus-download.sh"

HTTPS_ZIP_000 = (
    "https://digitalcorpora.s3.amazonaws.com/corpora/files/govdocs1/zipfiles/000.zip"
)
S3_ZIP_000 = "s3://digitalcorpora/corpora/files/govdocs1/zipfiles/000.zip"

failures: list[str] = []


def check(what: str, ok: bool, detail: str) -> None:
    if not ok:
        failures.append(f"{what}: {detail}")


def stub(directory: pathlib.Path, name: str, body: str) -> None:
    """An executable that stands in for a program this machine may not have."""
    path = directory / name
    path.write_text(f"#!/usr/bin/env bash\n{body}\n", encoding="utf-8")
    path.chmod(0o755)


def run(
    area: pathlib.Path,
    *arguments: str,
    with_aws: bool = False,
    with_curl: bool = True,
    environment: dict[str, str] | None = None,
    script: pathlib.Path | None = None,
) -> subprocess.CompletedProcess[str]:
    """The script, with a PATH holding exactly the programs the case wants.

    The real PATH is replaced rather than prepended to. Whether `aws` is
    installed is the thing under test, so a machine that happens to have one
    must not be able to turn a red case green -- which is what a prepended stub
    directory would allow for the "no aws" cases.
    """
    binaries = area / "bin"
    binaries.mkdir(exist_ok=True)
    # The script's own needs, from wherever this machine keeps them.
    for needed in ("bash", "date", "mkdir", "rm", "unzip", "seq", "basename",
                   "mv", "find", "wc", "tr", "ls", "dirname", "cd", "git"):
        found = shutil.which(needed)
        if found and not (binaries / needed).exists():
            (binaries / needed).symlink_to(found)
    if with_curl and not (binaries / "curl").exists():
        found = shutil.which("curl")
        if found:
            (binaries / "curl").symlink_to(found)
    if with_aws:
        stub(binaries, "aws", 'echo "stub aws $*" >&2; exit 1')

    env = dict(os.environ)
    env["PATH"] = str(binaries)
    env.pop("GOVDOCS_FETCHER", None)
    if environment:
        env.update(environment)

    return subprocess.run(
        ["bash", str(script or SCRIPT), *arguments],
        capture_output=True,
        text=True,
        env=env,
        cwd=area,
    )


def case_a_machine_without_aws(area: pathlib.Path) -> None:
    """The runner. This is the case that was broken and cost the corpus."""
    done = run(area, "--target", str(area / "out"), "--count", "200", "--dry-run")
    check("a machine without aws still has a fetcher", done.returncode == 0,
          f"exit {done.returncode}: {done.stderr[-400:]}")
    check("a machine without aws picks curl", "Fetcher:   curl" in done.stdout,
          f"stdout did not name curl: {done.stdout[-400:]}")
    check("a machine without aws reads the bucket over https",
          HTTPS_ZIP_000 in done.stdout,
          f"stdout did not name the https object: {done.stdout[-400:]}")


def case_a_machine_with_aws(area: pathlib.Path) -> None:
    """The CLI is preferred where it exists; this is a fallback, not a swap."""
    done = run(area, "--target", str(area / "out"), "--count", "200", "--dry-run",
               with_aws=True)
    check("a machine with aws keeps using it", "Fetcher:   aws" in done.stdout,
          f"stdout did not name aws: {done.stdout[-400:]}")
    check("a machine with aws reads the s3 object", S3_ZIP_000 in done.stdout,
          f"stdout did not name the s3 object: {done.stdout[-400:]}")


def case_aws_asked_for_and_absent(area: pathlib.Path) -> None:
    """Asking for a transport the machine does not have is an error, not a shrug."""
    done = run(area, "--target", str(area / "out"), "--count", "200", "--dry-run",
               environment={"GOVDOCS_FETCHER": "aws"})
    check("aws asked for and absent is refused", done.returncode != 0,
          "the run continued on a machine with no aws after aws was asked for")
    check("aws asked for and absent says so", "no aws on this machine" in done.stderr,
          f"stderr did not explain: {done.stderr[-400:]}")


def case_a_fetcher_nobody_has(area: pathlib.Path) -> None:
    done = run(area, "--target", str(area / "out"), "--count", "200", "--dry-run",
               environment={"GOVDOCS_FETCHER": "rsync"})
    check("an unknown fetcher is refused", done.returncode != 0,
          "an unknown fetcher was accepted and something else ran")
    check("an unknown fetcher is named", "Unknown fetcher" in done.stderr,
          f"stderr did not name it: {done.stderr[-400:]}")


def case_a_dry_run_promises_what_a_real_one_fetches(area: pathlib.Path) -> None:
    """Measured 07-09-2026: zip 000 holds 200 PDFs, and the estimate said 50.

    A dry run is how the size of a download is decided before it is started, so
    an estimate four times too low turns "25 zips, 20 minutes" into a plan for
    100. The count is pinned by the number of zips a dry run walks.
    """
    done = run(area, "--target", str(area / "out"), "--count", "400", "--dry-run")
    walked = done.stdout.count("[DRY-RUN] Would download:")
    check("a dry run walks the zips a real run would", walked == 2,
          f"400 documents at 200 per zip is 2 zips; the dry run walked {walked}")


def case_every_zip_unreadable(area: pathlib.Path) -> None:
    """A corpus that came up short is a failure, not a summary line (#276).

    And it is reached quickly. Ten zips in a row that cannot be read is a
    source that is gone, not a run of bad luck; walking the remaining 990 at
    three retries each spends an hour arriving at the same sentence.
    """
    binaries = area / "bin"
    binaries.mkdir(exist_ok=True)
    stub(binaries, "curl", "exit 7")
    done = run(area, "--target", str(area / "out"), "--count", "5", with_curl=False)
    check("a run that fetched nothing fails", done.returncode != 0,
          "a run that could not read one single zip reported success")
    check("a run that fetched nothing says how short it is",
          "SHORT (not a pass)" in done.stderr,
          f"stderr did not say it came up short: {done.stderr[-400:]}")
    check("an unreachable source is given up on", "GIVING UP" in done.stderr,
          f"stderr did not say it gave up: {done.stderr[-400:]}")
    check("an unreachable source is given up on after ten, not a thousand",
          "10 zip(s) unreadable" in done.stdout,
          f"stdout did not stop at ten: {done.stdout[-400:]}")


def case_the_suite_runner_fetches_rather_than_instructs(area: pathlib.Path) -> None:
    """`corpus-download.sh --suite govdocs` used to print a web page to visit.

    It made a directory, printed three manual steps and reported a skip, next
    to a script in the same folder that does the job unattended. "The corpus is
    reproducible from public sources" was true only of a person following
    instructions, which is a different claim from the one the gates rest on.

    The stub transport fails, so nothing is downloaded here; what is checked is
    that the suite runner reached the fetcher at all, and that a fetcher which
    produced nothing is reported as a failure by the runner too.
    """
    binaries = area / "bin"
    binaries.mkdir(exist_ok=True)
    stub(binaries, "curl", "exit 7")
    done = run(area, "--target", str(area / "out"), "--suite", "govdocs", "--count", "5",
               with_curl=False, script=SUITES)

    check("the suite runner delegates to the fetcher",
          "Delegating to corpus-download-govdocs.sh" in done.stdout,
          f"the govdocs suite did not call the fetcher: {done.stdout[-500:]}")
    check("the suite runner does not send a person to a web page",
          "digitalcorpora.org/corpora/files" not in done.stdout,
          f"the govdocs suite printed manual steps: {done.stdout[-500:]}")
    check("a fetcher that produced nothing fails the suite runner",
          done.returncode != 0,
          "corpus-download.sh reported success after the fetcher produced nothing")


def main() -> int:
    for needed in (SCRIPT, SUITES):
        if not needed.is_file():
            print(f"[test-govdocs] FATAL: {needed} is missing", file=sys.stderr)
            return 1
    if shutil.which("curl") is None:
        # Not a skip: every case here is about which transport is chosen, and a
        # machine with no curl cannot answer that question. Say so and fail.
        print("[test-govdocs] FATAL: no curl on this machine, so the transport "
              "this guard is about cannot be exercised", file=sys.stderr)
        return 1

    for case in (
        case_a_machine_without_aws,
        case_a_machine_with_aws,
        case_aws_asked_for_and_absent,
        case_a_fetcher_nobody_has,
        case_a_dry_run_promises_what_a_real_one_fetches,
        case_every_zip_unreadable,
        case_the_suite_runner_fetches_rather_than_instructs,
    ):
        area = pathlib.Path(tempfile.mkdtemp(prefix="govdocs-case."))
        try:
            case(area)
        finally:
            shutil.rmtree(area, ignore_errors=True)

    if failures:
        print("[test-govdocs] FATAL:", file=sys.stderr)
        for line in failures:
            print(f"  {line}", file=sys.stderr)
        return 1
    print("[test-govdocs] OK: the corpus can be fetched by a machine with no AWS CLI, "
          "the CLI is still preferred where it exists, and a run that comes up short "
          "fails instead of printing a summary line.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
