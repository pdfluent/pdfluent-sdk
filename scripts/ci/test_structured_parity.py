#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Prove the parity gate is red on every way it could be got past (#575).

The gate of #575 stands between a milestone and a release, and a gate like that
is worth what it refuses. So each way it could be made to pass without the work
having been done gets a case, driven against a sandbox whose run files this
file writes.

  an axis below the reference             the whole of what `--stable` is for.
                                          `--release` passes it on purpose --
                                          the preview publishes unless the
                                          round regressed (owner 2026-10-06).
  the reference edited down to ours       a target that moves toward whatever
                                          was achieved is not a target. The
                                          reference is checked against the run
                                          file it names, in this repository.
  a run worse than the last committed     the ratchet, which is what the push
                                          gate holds -- past its tolerance,
                                          because a gate that fires on the
                                          fourth decimal is a gate that gets
                                          switched off and then catches
                                          nothing.
  a ratchet tolerance as loose as
    parity's                              a ratchet compares us with ourselves
                                          over the same pages and has to fire
                                          on a fall parity would swallow.
  a column that matched fewer pages       the cheapest way to buy a mean: score
                                          the axis on the pages it is good at.
  an axis that stopped being scored       the same trick, spelt as absence.
  no reference file at all                a gate whose reference is missing must
                                          refuse, not pass.
  a reference with no set in it           the same, spelt as an empty file.
  the gate taken out of the lanes         a check that no lane runs is a check
                                          that will be true and unread. This is
                                          the case that goes red if the parity
                                          check is simply removed.

Exit codes:
  0  the gate is green where it should be and red on every mutation
  1  it is not
"""
from __future__ import annotations

import json
import statistics
import pathlib
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
GATE = "scripts/ci/loop/structured_parity.py"
REFERENCE = "benchmarks/structured/PARITY.toml"
RUNS = "benchmarks/runs/structured"
LANES = ("scripts/ci/local_ci_gate.sh", ".github/workflows/ci.yml")
# WHERE `--release` HAS TO BE CALLED, and its exit status obeyed. The lanes
# hold the ratchet and pass while the axes are below the reference, and since
# the owner decision of 06-10-2026 the publish paths do the same: the preview
# ships unless the newest master round regressed. Parity is what `--stable`
# holds, for the label -- and no publish path may call that spelling, or the
# preview would never leave. (codex, #1930)
# One preflight, and every publish path that could reach crates.io has to call
# it: the hosted workflow runs `publish_ordered.sh --live` directly and never
# touches the train guard, so a gate in the guard alone had a way round it.
PREFLIGHT = "scripts/release/the_structured_extraction_is_at_parity.sh"
RELEASE_PATHS = ("scripts/release/release_train_guard.sh",
                 "scripts/release/publish_ordered.sh")

# The reference of the fixture: round numbers, so that a reader of a failure
# can do the arithmetic in their head.
AXES = {"text": 0.600, "headings": 0.800, "lists": 0.700, "tables": 0.500,
        "order": 0.660, "order_geometric": 0.870}

FIXTURE = """
[meta]
tolerance = 0.005
ratchet_tolerance = 0.001
speed_share = 0.01

[sets.doclaynet]
run_id = "structured-doclaynet-20260101T000000Z"
tool = "docling"
tool_name = "Docling"
tool_version = "2.126.0"
pages = 200
half = "tuning"
start_up_amortised = true
ms_per_page = 2000.0

[sets.doclaynet.axes]
""" + "\n".join(f"{axis} = {value}" for axis, value in AXES.items()) + "\n"


def run_file(run_id: str, axes: dict, pages: dict | None = None,
             ms: float = 10.0, branch: str | None = None,
             harness: str | None = None, merge_base: str | None = None) -> dict:
    pages = pages or {}
    return {
        "run_id": run_id, "set": "doclaynet", "half": "tuning",
        "pages_measured": 200, "set_version": "fixture", "set_licence": "fixture",
        "machine": {"cpus": 8, "platform": "fixture"},
        # A round measured on a branch says which; one that names no line is
        # master's, which is what every round committed before the field is.
        # `harness` is the revision the round ran, at whatever width the rig
        # wrote it; a landing round measured at its fork names its merge-base
        # there, and that is what puts it on master's line.
        **({"harness": harness} if harness else {}),
        **({"line": {"branch": branch,
                     "merge_base": merge_base or "0000000"}} if branch else {}),
        "tools": {"pdfluent": {
            "name": "PDFluent", "version": "fixture", "ms_per_page": ms,
            "summary": {axis: {"mean": value, "pages": pages.get(axis, 100)}
                        for axis, value in axes.items()},
        }},
    }


def axis_run_disagreements(entry: dict, pinned: dict, other: dict) -> list[tuple]:
    """Where a run named in `axis_runs` is not the set's run on another axis.

    Each axis is read in the unit the set pins it in, as the pins are: an axis
    in `axis_units` as `pages` from the page summary, every other one from the
    reading the gate holds. Reading them all from one summary would pass a run
    whose page-level order differs so long as its document-level order agreed
    (codex, #1979).
    """
    sys.path.insert(0, str(REPO / "scripts" / "ci" / "loop"))
    import structured_parity as parity  # noqa: E402
    units = entry.get("axis_units") or {}
    skip = set(entry.get("axis_runs") or {})
    out = []
    pinned_reading, _ = parity.reading_of(pinned)
    other_reading, _ = parity.reading_of(other)
    for axis in entry.get("axes") or {}:
        if axis in skip:
            continue
        if units.get(axis) == "pages":
            a_summary, b_summary = pinned.get("summary") or {}, other.get("summary") or {}
        else:
            a_summary, b_summary = pinned_reading, other_reading
        a = (a_summary.get(axis) or {}).get("mean")
        b = (b_summary.get(axis) or {}).get("mean")
        if a is None or b is None or abs(float(a) - float(b)) > 1e-6:
            out.append((axis, a, b))
    return out


def build_sandbox(root: pathlib.Path) -> None:
    target = root / GATE
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(REPO / GATE, target)
    (root / REFERENCE).parent.mkdir(parents=True, exist_ok=True)
    (root / REFERENCE).write_text(FIXTURE)
    (root / RUNS).mkdir(parents=True, exist_ok=True)


def write_runs(root: pathlib.Path, runs: list[dict]) -> None:
    place = root / RUNS
    for old in place.glob("*.json"):
        old.unlink()
    for run in runs:
        (place / f"{run['run_id']}.json").write_text(json.dumps(run, indent=1))


def gate(root: pathlib.Path, *flags: str) -> tuple[int, str]:
    done = subprocess.run([sys.executable, str(root / GATE), *flags],
                          capture_output=True, text=True, check=False, cwd=root)
    return done.returncode, done.stdout + done.stderr


def main() -> int:
    failures: list[str] = []

    def fails(what: str, code: int, output: str, expect: str) -> None:
        if code == 0:
            failures.append(f"{what}: the gate stayed green\n{output}")
        elif expect and expect not in output:
            failures.append(f"{what}: red, but not about {expect!r}:\n{output}")

    def passes(what: str, code: int, output: str) -> None:
        if code != 0:
            failures.append(f"{what}: the gate was red\n{output}")

    # --- an axis run is held to the set's run in each axis's own unit -------
    # (codex, #1979) The same document-level order and a different page-level
    # order is another reference when the set pins order over pages.
    entry = {"axes": {"text": 0.6, "order": 0.8, "words": 0.9},
             "axis_units": {"order": "pages"}, "axis_runs": {"words": "x"}}
    def column(page_order: float, doc_order: float) -> dict:
        return {"summary": {"text": {"mean": 0.6}, "order": {"mean": page_order}},
                "in_documents": {"summary": {"text": {"mean": 0.6},
                                             "order": {"mean": doc_order}}}}
    if [d[0] for d in axis_run_disagreements(entry, column(0.8, 0.93), column(0.7, 0.93))] != ["order"]:
        failures.append("an axis run whose page-level order differs from the set's "
                        "passed, because its document-level order agreed")
    if axis_run_disagreements(entry, column(0.8, 0.93), column(0.8, 0.93)):
        failures.append("an axis run that agrees in every unit was refused")

    with tempfile.TemporaryDirectory(prefix="structured-parity-test-") as tmp:
        root = pathlib.Path(tmp)
        build_sandbox(root)

        at_parity = run_file("structured-doclaynet-20260102T000000Z", AXES, ms=5.0)
        write_runs(root, [at_parity])
        code, output = gate(root, "--release")
        passes("a run at the reference on every axis", code, output)
        code, output = gate(root, "--stable")
        passes("a run at the reference on every axis, for the stable label",
               code, output)

        # 1. one axis below the reference ----------------------------------
        low = dict(AXES, headings=AXES["headings"] - 0.05)
        write_runs(root, [run_file("structured-doclaynet-20260102T000000Z", low,
                                   ms=5.0)])
        code, output = gate(root, "--release")
        passes("an axis five points below the reference, for a publish "
               "(preview: the publish gate holds the ratchet, not parity)",
               code, output)
        code, output = gate(root, "--stable")
        fails("an axis five points below the reference", code, output, "PARITY")

        # And the push gate does NOT fail on it, which is the whole reason the
        # two verdicts are two flags: a gate that is red for a reason nobody
        # can fix today is a gate people learn to push past.
        code, output = gate(root, "--check")
        passes("the push gate on the same run", code, output)

        # 1b. an ADVISORY axis is printed and pinned but judges nothing -----
        # A new axis starts advisory (T6, #1973): below the reference it must
        # not fail the release, and a fall between two runs must not fail the
        # push -- while the same fall on an axis that is not advisory still
        # does, which is what shows the list and not the fixture decides it.
        advisory = FIXTURE.replace(
            "speed_share = 0.01\n",
            "speed_share = 0.01\nadvisory_axes = [\"words\"]\n") + "words = 0.950\n"
        judged = FIXTURE + "words = 0.950\n"
        words_low = dict(AXES, words=0.900)
        write_runs(root, [run_file("structured-doclaynet-20260102T000000Z",
                                   words_low, ms=5.0)])
        (root / REFERENCE).write_text(advisory)
        code, output = gate(root, "--stable")
        passes("an advisory axis five points below the reference", code, output)
        (root / REFERENCE).write_text(judged)
        code, output = gate(root, "--stable")
        fails("the same axis once it is no longer advisory", code, output, "PARITY")
        # And a run taken before the axis existed, which has no figure for it
        # at all: the first round after the axis lands is still one of these.
        write_runs(root, [run_file("structured-doclaynet-20260102T000000Z", AXES, ms=5.0)])
        (root / REFERENCE).write_text(advisory)
        code, output = gate(root, "--stable")
        passes("a run that predates the advisory axis", code, output)
        # Nor when the run is read over documents and the pin over pages: the
        # judged axes fail on that, the advisory one reports it.
        regrouped = run_file("structured-doclaynet-20260102T000000Z", AXES, ms=5.0)
        column = regrouped["tools"]["pdfluent"]
        column["in_documents"] = {"read_as": "the document, fed to the tool",
                                  "documents": 10,
                                  "summary": dict(column["summary"],
                                                  words={"mean": 0.9, "pages": 10})}
        write_runs(root, [regrouped])
        code, output = gate(root, "--release")
        if "| text (words, order-free) |" in output and "advisory" not in output.split("| text (words, order-free) |")[1].split("\n")[0]:
            failures.append("an advisory axis read in another unit than its pin "
                            f"was judged:\n{output}")
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z", dict(AXES, words=0.950), ms=5.0),
            run_file("structured-doclaynet-20260103T000000Z", dict(AXES, words=0.940), ms=5.0)])
        (root / REFERENCE).write_text(advisory)
        code, output = gate(root, "--check")
        passes("an advisory axis a point below the last run", code, output)
        # Nor when it is scored on fewer units, or not at all this time
        # (codex, #1976): each would be a REGRESSION on a judged axis.
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z", dict(AXES, words=0.950), ms=5.0),
            run_file("structured-doclaynet-20260103T000000Z", dict(AXES, words=0.950),
                     pages={"words": 90}, ms=5.0)])
        code, output = gate(root, "--check")
        passes("an advisory axis scored on fewer units", code, output)
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z", dict(AXES, words=0.950), ms=5.0),
            run_file("structured-doclaynet-20260103T000000Z", AXES, ms=5.0)])
        code, output = gate(root, "--check")
        passes("an advisory axis this round did not score", code, output)
        (root / REFERENCE).write_text(judged)
        code, output = gate(root, "--check")
        fails("the same fall once the axis is no longer advisory", code, output,
              "RATCHET")
        (root / REFERENCE).write_text(FIXTURE)

        # 2. the reference edited down to what was achieved -----------------
        # Not a sandbox case: the reference in this repository has to be the
        # numbers of the run it names, and that is checked against the run file
        # itself, below.

        # 3. a run worse than the last committed one ------------------------
        # And a fall inside the ratchet tolerance first, which must NOT be red:
        # a gate that fires on the fourth decimal is a gate that gets switched
        # off, and then the case below stops being caught at all.
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z", AXES, ms=5.0),
            run_file("structured-doclaynet-20260103T000000Z",
                     dict(AXES, lists=AXES["lists"] - 0.0004), ms=5.0)])
        code, output = gate(root, "--check")
        passes("a fall of four ten-thousandths over the same pages", code, output)
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z", AXES, ms=5.0),
            run_file("structured-doclaynet-20260103T000000Z",
                     dict(AXES, lists=AXES["lists"] - 0.01), ms=5.0)])
        code, output = gate(root, "--check")
        fails("a committed run below the last committed run", code, output,
              "RATCHET")

        # 4. a column that matched fewer pages ------------------------------
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z", AXES, ms=5.0),
            run_file("structured-doclaynet-20260103T000000Z",
                     dict(AXES, lists=0.99), pages={"lists": 10}, ms=5.0)])
        code, output = gate(root, "--check")
        fails("an axis scored on fewer pages, with a better mean", code, output,
              "RATCHET")

        # 5. an axis that stopped being scored -------------------------------
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z", AXES, ms=5.0),
            run_file("structured-doclaynet-20260103T000000Z",
                     {a: v for a, v in AXES.items() if a != "lists"}, ms=5.0)])
        code, output = gate(root, "--check")
        fails("an axis the last run scored and this one does not", code, output,
              "RATCHET")

        # And an axis that reached MORE pages is not a regression, however the
        # mean moved: the two means are over different populations.
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z", AXES,
                     pages={"order": 17}, ms=5.0),
            run_file("structured-doclaynet-20260103T000000Z",
                     dict(AXES, order=0.40), pages={"order": 149}, ms=5.0)])
        code, output = gate(root, "--check")
        passes("an axis that reached more pages with a lower mean", code, output)
        if "incomparable" not in output:
            failures.append("the wider denominator was not reported as "
                            "incomparable, so a reader cannot see why it passed")

        # 6. the speed ceiling ----------------------------------------------
        write_runs(root, [run_file("structured-doclaynet-20260102T000000Z",
                                   AXES, ms=21.0)])
        code, output = gate(root, "--release")
        passes("ms/page above one percent of the reference, for a publish "
               "(preview)", code, output)
        code, output = gate(root, "--stable")
        fails("ms/page above one percent of the reference", code, output, "PARITY")

        # 7. no reference file, and an empty one -----------------------------
        write_runs(root, [at_parity])
        kept = (root / REFERENCE).read_text()
        (root / REFERENCE).unlink()
        code, output = gate(root, "--check")
        fails("no reference file at all", code, output, "not there")
        (root / REFERENCE).write_text("[meta]\ntolerance = 0.005\n")
        code, output = gate(root, "--check")
        fails("a reference file with no set in it", code, output, "names no set")
        (root / REFERENCE).write_text(kept)

        # 8. a smoke run, and a holdout run, as the newest committed file -----
        smoke = run_file("structured-doclaynet-20260104T000000Z", AXES, ms=5.0)
        smoke["pages_measured"] = 1
        write_runs(root, [at_parity, smoke])
        code, output = gate(root, "--release")
        fails("a one-page smoke run as the newest committed run", code, output,
              "cannot be read against the reference")
        code, output = gate(root, "--check")
        fails("and the push gate does not pass it either", code, output,
              "cannot be read against the reference")
        spent = run_file("structured-doclaynet-20260104T000000Z", AXES, ms=5.0)
        spent["half"] = "because the release measurement needs it"
        write_runs(root, [at_parity, spent])
        code, output = gate(root, "--release")
        fails("a holdout run as the newest committed run", code, output,
              "cannot be read against the reference")

        # 9. two rescores of one round, the newer one named by a smaller hash --
        write_runs(root, [at_parity])
        place = root / RUNS
        for name, when, headings in (
                ("structured-doclaynet-20260102T000000Z-rescored-zzzzzzz",
                 "2026-01-03T00:00:00+00:00", AXES["headings"] - 0.2),
                ("structured-doclaynet-20260102T000000Z-rescored-aaaaaaa",
                 "2026-01-04T00:00:00+00:00", AXES["headings"])):
            row = run_file(name, dict(AXES, headings=headings), ms=5.0)
            row["rescored_from"] = "structured-doclaynet-20260102T000000Z"
            row["when"] = when
            (place / f"{name}.json").write_text(json.dumps(row, indent=1))
        code, output = gate(root, "--release")
        passes("the newest rescore by its clock, not by its git hash", code,
               output)
        if "rescored-aaaaaaa" not in output:
            failures.append(
                "the older rescore was read: a rescore id ends in a git hash, "
                "so sorting the names hands the gate a stale scoring")

        # 10. a run file with no column of ours -------------------------------
        headless = run_file("structured-doclaynet-20260102T000000Z", AXES)
        headless["tools"] = {"docling": headless["tools"]["pdfluent"]}
        write_runs(root, [headless])
        code, output = gate(root, "--check")
        fails("a run file with no column of ours", code, output, "no `pdfluent`")

        # 11. a branch's round beside master's line ---------------------------
        # Two terminals measure two rounds an hour apart on two branches, and
        # the later one is not the successor of the earlier: it is its sibling,
        # measured on a tree that never held the other's fix. Read as a
        # sequence, the stronger sibling turns the weaker into a regression
        # nobody can repair, and it sits on master for good. (#593)
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z",
                     dict(AXES, lists=0.430), ms=5.0),
            run_file("structured-doclaynet-20260103T000000Z",
                     dict(AXES, lists=0.520), ms=5.0, branch="t3/markers"),
            run_file("structured-doclaynet-20260104T000000Z",
                     dict(AXES, lists=0.434), ms=5.0)])
        code, output = gate(root, "--check")
        passes("master's line read past a branch's round", code, output)
        if "beside the line" not in output:
            failures.append("a skipped round is not named in the output:\n"
                            + output)
        # And master's own line still bites, with the branch round beside it.
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z",
                     dict(AXES, lists=0.520), ms=5.0),
            run_file("structured-doclaynet-20260103T000000Z",
                     dict(AXES, lists=0.600), ms=5.0, branch="t3/markers"),
            run_file("structured-doclaynet-20260104T000000Z",
                     dict(AXES, lists=0.434), ms=5.0)])
        code, output = gate(root, "--check")
        fails("a fall along master's own line, with a branch round beside it",
              code, output, "RATCHET")
        # And the NEWEST round being a branch's does not make it the round the
        # gate judges: the verdict is about master's line, so the branch round
        # is printed beside and master's own last round is read. A branch that
        # measured its own tree below the reference would otherwise fail a gate
        # about a tree master is not. (#593)
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z", AXES, ms=5.0),
            run_file("structured-doclaynet-20260105T000000Z",
                     dict(AXES, lists=0.100, text=0.100), ms=900.0,
                     branch="t3/markers")])
        code, output = gate(root, "--release")
        passes("a branch's round as the newest committed run", code, output)

        # 11b. a landing round measured at its fork IS master's line ---------
        # A landing branch carries a train onto master, and a round taken on
        # one while it sits exactly at its merge-base measured the tree
        # master already is. The rig writes the fork point at nine digits and
        # the harness at eight, so the fixture does that too.
        fork, measured = "abcdef123", "abcdef12"
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z", AXES, ms=5.0),
            run_file("structured-doclaynet-20260103T000000Z", AXES, ms=5.0,
                     branch="landing/train/x-20260103", harness=measured,
                     merge_base=fork)])
        code, output = gate(root, "--release")
        passes("a landing round measured at its fork", code, output)
        if "### parity: structured-doclaynet-20260103T000000Z" not in output:
            failures.append("the landing round was stepped past, so the gate "
                            f"judged the older master round instead:\n{output}")
        # And it is really ON the line: a fall there is a regression, not a
        # sibling the gate reads past.
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z", AXES, ms=5.0),
            run_file("structured-doclaynet-20260103T000000Z",
                     dict(AXES, lists=AXES["lists"] - 0.01), ms=5.0,
                     branch="landing/train/x-20260103", harness=measured,
                     merge_base=fork)])
        code, output = gate(root, "--check")
        fails("a fall on a landing round measured at its fork", code, output,
              "RATCHET")
        # But a landing branch that measured PAST its fork is a branch like
        # any other: the tree it ran is not the tree master is.
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z", AXES, ms=5.0),
            run_file("structured-doclaynet-20260105T000000Z",
                     dict(AXES, lists=0.100, text=0.100), ms=900.0,
                     branch="landing/train/x-20260105", harness="12345678",
                     merge_base=fork)])
        code, output = gate(root, "--release")
        passes("a landing round measured past its fork", code, output)
        if "beside the line" not in output:
            failures.append("a landing round past its fork was not stepped "
                            f"past:\n{output}")
        # And a rescore takes its round's line: its own harness is the
        # harness that re-scored it, which names no measured tree.
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z", AXES, ms=5.0),
            run_file("structured-doclaynet-20260103T000000Z", AXES, ms=5.0,
                     branch="landing/train/x-20260103", harness=measured,
                     merge_base=fork)])
        rescore = run_file(
            "structured-doclaynet-20260103T000000Z-rescored-beef123", AXES,
            ms=5.0, branch="landing/train/x-20260103", harness="beef1234",
            merge_base=fork)
        rescore["rescored_from"] = "structured-doclaynet-20260103T000000Z"
        rescore["when"] = "2026-01-04T00:00:00+00:00"
        (root / RUNS / f"{rescore['run_id']}.json").write_text(
            json.dumps(rescore, indent=1))
        code, output = gate(root, "--release")
        passes("a rescore of a landing round measured at its fork", code,
               output)
        if "### parity: structured-doclaynet-20260103T000000Z-rescored-beef123" \
                not in output:
            failures.append("the rescore did not take its round's line, so "
                            "the gate judged the older master round "
                            f"instead:\n{output}")

        # 12. a page reading judged against a pin recorded over documents -----
        # The document pass can fail on its own -- no `mutool`, or every
        # assembled document refused -- and then the column carries a `why`
        # where its document summary would be and the gate falls back to the
        # page reading. On these axes a page scores HIGHER than the document it
        # belongs to, so the verdict such a run would otherwise get is a PASS
        # for a question nobody asked. (codex, #1944)
        (root / REFERENCE).write_text(FIXTURE.replace(
            "start_up_amortised = true",
            "start_up_amortised = true\nreading = \"documents\""))
        write_runs(root, [run_file("structured-doclaynet-20260102T000000Z",
                                   AXES, ms=5.0)])
        code, output = gate(root, "--release")
        passes("a page reading against a pin over documents, for a publish "
               "(preview)", code, output)
        code, output = gate(root, "--stable")
        fails("a page reading against a pin over documents", code, output,
              "pinned over documents")
        refused = run_file("structured-doclaynet-20260102T000000Z", AXES, ms=5.0)
        refused["tools"]["pdfluent"]["in_documents"] = {
            "why": "mutool is not on this machine"}
        write_runs(root, [refused])
        code, output = gate(root, "--release")
        passes("a column whose document pass was refused, for a publish "
               "(preview)", code, output)
        code, output = gate(root, "--stable")
        fails("a column whose document pass was refused", code, output,
              "pinned over documents")
        # And the clock is ratcheted across the reading all the same: a round
        # that changes the unit may not also become ten times slower unseen.
        pages_first = run_file("structured-doclaynet-20260102T000000Z", AXES,
                               ms=5.0)
        documents_now = run_file("structured-doclaynet-20260103T000000Z", AXES,
                                 ms=50.0)
        documents_now["tools"]["pdfluent"]["in_documents"] = {
            "summary": {axis: {"mean": value, "pages": 11}
                        for axis, value in AXES.items()}}
        write_runs(root, [pages_first, documents_now])
        code, output = gate(root, "--check")
        fails("a tenfold clock across a change of reading", code, output,
              "RATCHET")
        (root / REFERENCE).write_text(FIXTURE)

        # 13. an axis pinned in its own unit ----------------------------------
        # The order axes are pinned over pages while the rest are pinned over
        # documents: assembling a document invents pairs of blocks that no tool
        # ever puts the wrong way round, so the axis rises without anything
        # being read better. A gate that read the document figure for them
        # would hold us to a bar made of page numbering. (#584, decided 23-09)
        (root / REFERENCE).write_text(FIXTURE.replace(
            "start_up_amortised = true",
            "start_up_amortised = true\nreading = \"documents\"\n"
            "axis_units = { order = \"pages\" }"))
        two_readings = run_file("structured-doclaynet-20260102T000000Z",
                                AXES, ms=5.0)
        two_readings["tools"]["pdfluent"]["in_documents"] = {
            "documents": 11,
            "summary": {axis: {"mean": (0.100 if axis == "order" else value),
                               "pages": 11}
                        for axis, value in AXES.items()},
        }
        write_runs(root, [two_readings])
        code, output = gate(root, "--release")
        passes("an axis read in the unit its pin names", code, output)
        (root / REFERENCE).write_text(FIXTURE)

    # --- the reference in THIS repository is the run it names ---------------
    failures.extend(reference_is_its_run())

    # --- and the lanes run it ----------------------------------------------
    failures.extend(the_lanes_run_it())
    failures.extend(the_release_path_holds_it())

    if failures:
        print(f"The parity gate: {len(failures)} problem(s)\n", file=sys.stderr)
        for line in failures:
            print(f"  - {line}", file=sys.stderr)
        return 1
    print("✓ the parity gate refuses an axis below the reference, a run below "
          "the last committed one, a column that bought its mean by matching "
          "fewer pages, a missing reference and a lane that stopped running it")
    return 0


def reference_is_its_run() -> list[str]:
    """Every pinned number is the number in the run file the reference names.

    The one mutation a sandbox cannot test, because it is a change to the
    committed reference rather than to a run: lower the bar to what was
    achieved and every axis passes. The defence is that the reference says
    which run it came from, and that run is in this repository.
    """
    sys.path.insert(0, str(REPO / "scripts" / "ci" / "loop"))
    import structured_parity as parity  # noqa: E402

    problems: list[str] = []
    try:
        reference = parity.load_reference()
    except SystemExit as exc:
        return [f"the committed reference does not load: {exc}"]

    meta = reference["meta"]
    if not meta.get("ratchet_tolerance"):
        problems.append(
            "the reference names no `ratchet_tolerance`, so the ratchet fires "
            "on the fourth decimal and will be switched off")
    elif float(meta["ratchet_tolerance"]) >= float(meta.get("tolerance", 0)):
        problems.append(
            f"the ratchet tolerance ({meta['ratchet_tolerance']}) is not "
            f"tighter than parity's ({meta.get('tolerance')}). A ratchet "
            "compares us with ourselves over the same pages and has to fire "
            "on a fall parity would swallow.")
    for name, entry in reference["sets"].items():
        path = parity.RUNS / f"{entry['run_id']}.json"
        if not path.is_file():
            problems.append(
                f"set `{name}` pins run {entry['run_id']}, which is not in "
                f"{parity.RUNS.relative_to(REPO)}. A reference whose run is not "
                "in the tree is a number nobody can check.")
            continue
        run = json.loads(path.read_text())
        column = (run.get("tools") or {}).get(entry["tool"])
        if column is None:
            problems.append(
                f"set `{name}` pins the `{entry['tool']}` column of "
                f"{entry['run_id']}, which has no such column.")
            continue
        # THE PINS ARE OF THE READING THE GATE HOLDS (#584). A reference that
        # pins the document reading and is checked against the page reading of
        # the same run would pass on numbers the gate never reads, which is the
        # one thing this guard exists to stop.
        pinned_summary, pinned_reading = parity.reading_of(column)
        if entry.get("reading", "pages") != pinned_reading:
            problems.append(
                f"set `{name}` says its reference is read over "
                f"{entry.get('reading', 'pages')} and {entry['run_id']} offers "
                f"{pinned_reading}. A reference and a gate that read two "
                "different units are not a comparison.")
        # AND AN AXIS MAY NAME ITS OWN UNIT (#584, decided 23-09). The order
        # axes are pinned over pages while the rest are pinned over documents,
        # because assembling a document invents pairs no tool gets wrong; the
        # file says so in `axis_units` and this reads the pin from the summary
        # that unit names, or the pin would be checked against a number the
        # gate never reads.
        units = entry.get("axis_units") or {}
        # AN AXIS MAY NAME ITS OWN RUN (#1973): one added later is pinned from
        # a rescore of a round whose figures on every other pinned axis are the
        # pinned run's own, without moving the run -- or its clock -- that the
        # rest of the set stands on. It is held to that run, and the run has to
        # agree with the set's on every other axis, or it is another reference.
        axis_runs = entry.get("axis_runs") or {}
        for axis, other_id in axis_runs.items():
            other_path = parity.RUNS / f"{other_id}.json"
            if not other_path.is_file():
                problems.append(f"set `{name}` pins {axis} from run {other_id}, "
                                "which is not in the tree")
                continue
            other_column = (json.loads(other_path.read_text()).get("tools") or {}).get(entry["tool"]) or {}
            for same_axis, a, b in axis_run_disagreements(entry, column, other_column):
                problems.append(
                    f"set `{name}` pins {axis} from {other_id}, whose "
                    f"{same_axis} ({b}) is not the pinned run's ({a}): that "
                    "run is another reference, not the same one rescored")
        for axis, want in (entry.get("axes") or {}).items():
            source_id = axis_runs.get(axis, entry["run_id"])
            source_column = column
            source_summary = pinned_summary
            if source_id != entry["run_id"]:
                source_path = parity.RUNS / f"{source_id}.json"
                if not source_path.is_file():
                    continue
                source_column = (json.loads(source_path.read_text()).get("tools") or {}).get(entry["tool"]) or {}
                source_summary, _ = parity.reading_of(source_column)
            summary = source_summary
            if units.get(axis) == "pages":
                summary = source_column.get("summary") or {}
            got = (summary.get(axis) or {}).get("mean")
            if got is None or abs(float(got) - float(want)) > 1e-6:
                problems.append(
                    f"set `{name}` pins {axis} = {want} and {source_id} "
                    f"recorded {got}. The reference is the run, not a figure "
                    "beside it.")
        # THE CLOCK IS PINNED AS A MEDIAN OVER THE REFERENCE'S ROUNDS, not as
        # the figure of the run the axes come from (#584). Docling's own clock
        # moved 30 % between two rounds with nothing changing in it, and a
        # ceiling taken from whichever round was last is a ceiling that fails
        # our speed on the machine's load. Recomputed here from the run files,
        # so a pin taken from one round goes red rather than standing.
        if entry.get("ms_per_page_rule"):
            clocks = []
            # A CACHED COLUMN IS ONE MEASUREMENT, however many rounds read it
            # (#623): every round since 26-09 scores the reference from the
            # competitor cache, and counting its one clock once per round would
            # let the cache outvote every round that measured it.
            cached_seen: set[str] = set()
            for path in sorted(parity.RUNS.glob(f"structured-{name}-*.json")):
                if "-rescored-" in path.name:
                    continue
                other = json.loads(path.read_text())
                other_column = (other.get("tools") or {}).get(entry["tool"]) or {}
                # `cached_to` in the round that measured and stored it,
                # `from_cache.file` in every round that read it back: one
                # measurement under two keys (codex, #2005).
                cached = ((other_column.get("from_cache") or {}).get("file")
                          or other_column.get("cached_to"))
                if cached:
                    if cached in cached_seen:
                        continue
                    cached_seen.add(cached)
                if other_column.get("ms_per_page"):
                    clocks.append(float(other_column["ms_per_page"]))
            want = statistics.median(clocks) if clocks else None
            if want is None or abs(want - float(entry["ms_per_page"])) > 1e-6:
                problems.append(
                    f"set `{name}` pins {entry['ms_per_page']} ms/page and the "
                    f"median over the {len(clocks)} committed round(s) of the "
                    f"`{entry['tool']}` column is {want}. The rule the file "
                    "states is the one the gate has to be able to recompute.")
        elif abs(float(column.get("ms_per_page", -1)) - float(entry["ms_per_page"])) > 1e-6:
            problems.append(
                f"set `{name}` pins {entry['ms_per_page']} ms/page and the run "
                f"recorded {column.get('ms_per_page')}.")
        amortised = bool(entry.get("start_up_amortised"))
        if bool(column.get("start_up_amortised")) != amortised:
            problems.append(
                f"set `{name}` says the reference's start-up was "
                f"{'amortised' if amortised else 'not amortised'} and the run "
                "says the other thing. An unlabelled clock across two columns "
                "is not a comparison.")
        pinned = entry.get("furniture") or {}
        if pinned:
            leaked = sum(d["furniture"]["in_the_body"] for d in column["documents"])
            annotated = sum(d["furniture"]["annotated"] for d in column["documents"])
            if (leaked, annotated) != (pinned.get("in_the_body"),
                                       pinned.get("annotated")):
                problems.append(
                    f"set `{name}` pins furniture {pinned.get('in_the_body')} of "
                    f"{pinned.get('annotated')} and the run counted {leaked} of "
                    f"{annotated}.")
            in_document = column.get("furniture_in_the_document") or {}
            want = (pinned.get("in_the_document"),
                    pinned.get("in_the_document_annotated"))
            # A RESCORE CANNOT TAKE THIS FIGURE, and asking it to would make the
            # rescore mechanism unusable for a set that counts furniture. The
            # count with the pages back in their documents needs the tool to
            # read a whole document, which needs the machine a rescore exists to
            # do without -- `rescore` says so in the column itself. The figure
            # stays pinned, because it is a fact about the reference that a
            # ROUND measured; what is skipped is the demand that a re-scoring of
            # that round reproduce it. The body count above is checked as
            # before, on both. (#577)
            rescored = bool(run.get("rescored_from")) and "why" in in_document
            if any(v is not None for v in want) and not rescored:
                got = (in_document.get("in_the_body"),
                       in_document.get("annotated"))
                if got != want:
                    problems.append(
                        f"set `{name}` pins an in-document furniture count of "
                        f"{want[0]} of {want[1]} and the run counted {got[0]} "
                        f"of {got[1]}. The figure that measures #571's rule is "
                        "the one with the pages back in their documents.")
    return problems


def the_lanes_run_it() -> list[str]:
    """Both lanes call the gate. Take it out of them and this goes red.

    A gate nothing runs is a sentence in a file. The push and landing lanes are
    `local_ci_gate.sh`; the hosted lane is `ci.yml`; and #575's sentence is
    that a regression in a committed run is a red gate rather than a comment,
    which is only true while both of them ask.
    """
    problems: list[str] = []
    for lane in LANES:
        text = (REPO / lane).read_text()
        after = text.split("structured_parity.py", 1)[-1][:200] if \
            "structured_parity.py" in text else ""
        if "--set" in after.split("\n")[0]:
            problems.append(
                f"{lane} runs {GATE} for ONE set. Each set answers the axes it "
                "draws, so a lane pinned to one holds the ratchet on half the "
                "measurement and is green about the other half.")
        if "structured_parity.py" not in text:
            problems.append(
                f"{lane} does not run {GATE}. A parity check no lane runs is a "
                "check that is true and unread.")
            continue
        if "--check" not in text.split("structured_parity.py", 1)[1][:200]:
            problems.append(
                f"{lane} runs {GATE} without `--check`, so it prints a table "
                "and holds nothing.")
    return problems


def the_release_path_holds_it() -> list[str]:
    """Every publish path refuses on a regression, through one preflight.

    The lanes run `--check`, which passes on purpose while the axes are below
    the reference, and since the owner decision of 06-10-2026 the publish
    paths do the same through the preflight below: the preview ships unless
    the newest master round regressed. `--stable` is the verdict parity is --
    the condition for the stable label and for any parity claim -- and NO
    publish path may call it, or the preview would never leave. The preflight
    was first written into the train guard alone, and the hosted crates.io
    workflow runs `publish_ordered.sh --live` directly -- so the rule sat in
    a parallel orchestrator the live path never calls. (codex, #1930)
    """
    problems: list[str] = []
    preflight = REPO / PREFLIGHT
    if not preflight.is_file():
        return [f"{PREFLIGHT} is not there, so nothing holds the owner's "
                "publish rule."]
    text = preflight.read_text()
    calls = [line for line in text.splitlines()
             if "--release" in line and not line.lstrip().startswith("#")
             and ("structured_parity.py" in line or "${POORT}" in line)]
    if not calls:
        problems.append(
            f"{PREFLIGHT} never runs {GATE} with `--release`, so a publish "
            "goes ahead on a regression -- and the ratchet is all the "
            "preview's publish path holds.")
    elif not calls[0].lstrip().startswith("if !") or \
            "exit " not in text[text.index(calls[0]):text.index(calls[0]) + 900]:
        problems.append(
            f"{PREFLIGHT} runs {GATE} without exiting on its verdict. A call "
            "whose status is discarded is not a gate -- which is how this was "
            "missed in the benchmark round, where it ran under `set +e`.")
    for path in (PREFLIGHT, *RELEASE_PATHS):
        stable = [line for line in (REPO / path).read_text().splitlines()
                  if "--stable" in line and not line.lstrip().startswith("#")]
        if stable:
            problems.append(
                f"{path} calls `--stable`. That spelling holds parity, which "
                "is the condition for the stable label -- not for a publish. "
                "A publish path that calls it ships nothing until parity, "
                "which unships the preview the owner decided on 06-10-2026.")

    name = pathlib.PurePath(PREFLIGHT).name
    for path in RELEASE_PATHS:
        body = (REPO / path).read_text()
        runs = [line for line in body.splitlines()
                if name in line and not line.lstrip().startswith("#")]
        if not runs:
            problems.append(
                f"{path} does not call {name}. A publish path that skips the "
                "preflight is a way round the owner's rule, and there is more "
                "than one publish path.")
            continue
        if not runs[0].lstrip().startswith("if !"):
            problems.append(
                f"{path} calls {name} without exiting on its verdict.")
    return problems


if __name__ == "__main__":
    sys.exit(main())
