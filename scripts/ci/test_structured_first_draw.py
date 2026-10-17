#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""FinTabNet's first draw can happen on the rig (T6, #574).

Round 5 fetched 300 FinTabNet pages and died at `--draw`: `draw()` writes to
`manifest_for(set)`, which only knows sealed sets, while the guard exits 3
for a registered set without a committed manifest -- so the row the draw
needs is the row that cannot be added before the draw. And the lane's own
self-test never drew, so the lane went red on a paid machine instead of on
a push.

The rule under test: a set whose seed exists and whose first draw has not
landed yet sits in FIRST_DRAW, `draw_path_for` writes and reads it there,
and the guard still sees only MANIFESTS. Six cases, and each one is red
when its rule is removed:

  1. `draw_path_for` gives the FIRST_DRAW path for fintabnet, the MANIFESTS
     path for a sealed set, and the existing FATAL for an unknown one --
     while `manifest_for` still refuses fintabnet.
  2. the bootstrap end to end: `draw("fintabnet", ...)` on 10 synthetic
     pages seals under the fintabnet seed, and `_drawn_on` keeps the draw
     date of a file with the same seal.
  3. no set is in both tables, and no committed file sits under FIRST_DRAW.
  4. the guard is unchanged for FIRST_DRAW sets: exit 0 on this tree.
  5. the lane's self-test runs the draw.
  6. `main()` with `--measure` reads the manifest through `draw_path_for`,
     so the FinTabNet round measures the draw it has just sealed.

Exit codes:
  0  the first draw can happen on the rig
  1  it cannot
"""
from __future__ import annotations

import contextlib
import datetime as dt
import hashlib
import io
import json
import pathlib
import re
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "ci"))
sys.path.insert(0, str(REPO / "scripts" / "ci" / "loop"))

import structured_holdout_rule as rule  # noqa: E402
import structured_bench as bench  # noqa: E402

GUARD = REPO / "scripts" / "ci" / "the_structured_holdout_is_the_drawn_set.py"
BENCH = REPO / "scripts" / "ci" / "loop" / "structured_bench.py"


def draw_path_is_the_drawn_path() -> list[str]:
    """`draw_path_for` routes sealed sets and first draws apart."""
    failures: list[str] = []
    try:
        got = rule.draw_path_for("fintabnet")
    except SystemExit as exc:
        failures.append(f"draw_path_for refused fintabnet: {exc} -- round 5's "
                        "FATAL, and the first draw needs this row.")
        return failures
    first = rule.FIRST_DRAW.get("fintabnet")
    if first is None:
        failures.append("fintabnet has no FIRST_DRAW row -- the first draw "
                        "needs one until the commit that lands the manifest "
                        "moves it to MANIFESTS with the ledger row.")
    else:
        if got != first:
            failures.append(f"draw_path_for(fintabnet) is {got}, not the "
                            "FIRST_DRAW path -- the round would seal "
                            "somewhere the evidence does not carry.")
        want = REPO / "corpus" / "structured" / "FINTABNET.json"
        if first != want:
            failures.append(f"the FIRST_DRAW path for fintabnet is {first}, "
                            f"not {want.relative_to(REPO)} -- the evidence "
                            "tarball carries `corpus/structured`, and only "
                            "that.")
    try:
        sealed = rule.draw_path_for("doclaynet")
    except SystemExit as exc:
        failures.append(f"draw_path_for refused a sealed set: {exc}")
    else:
        sealed_want = rule.MANIFESTS.get("doclaynet")
        if sealed_want is None:
            failures.append("doclaynet has no MANIFESTS row -- a sealed set "
                            "must keep the path it always drew to.")
        elif sealed != sealed_want:
            failures.append(f"draw_path_for(doclaynet) is {sealed}, not the "
                            "MANIFESTS path -- a sealed set must keep drawing "
                            "where it always drew.")
    try:
        rule.draw_path_for("no-such-set")
        failures.append("draw_path_for accepted an unknown set -- a set drawn "
                        "under a seed this file does not name is a split "
                        "nobody else can redraw.")
    except SystemExit as exc:
        if "FATAL" not in str(exc):
            failures.append(f"an unknown set was refused, but not with the "
                            f"existing FATAL: {exc}")
    except Exception as exc:
        failures.append(f"an unknown set raised {type(exc).__name__} instead "
                        f"of the existing FATAL: {exc}")
    try:
        rule.manifest_for("fintabnet")
        failures.append("manifest_for stopped refusing fintabnet -- the guard "
                        "and every reader of a sealed set must still see only "
                        "MANIFESTS.")
    except SystemExit:
        pass
    return failures


def synthetic_pages(count: int = 10) -> list[dict]:
    """Ten pages with the shape a draw reads, one document each.

    One page per document, so the seeded walk holds two pages back: a
    fixture that drew an empty holdout would pass every seal check below
    for the wrong reason.
    """
    return [{
        "source": "fintabnet/ibm-1.0.0",
        "name": f"fixture{i:02d}.pdf",
        "document": f"company{i:02d}_2020",
        "page_no": 0,
        "category": "financial reports",
        "sha256": hashlib.sha256(f"first-draw-page{i}".encode()).hexdigest(),
    } for i in range(count)]


def the_bootstrap_seals() -> list[str]:
    """The bootstrap end to end, into a scratch directory."""
    failures: list[str] = []
    entry = {"version": "first-draw fixture", "revision": "abc123",
             "source": "fixture", "licence": "CDLA-Permissive-1.0"}
    pages = synthetic_pages()
    with tempfile.TemporaryDirectory(prefix="structured-first-draw-") as tmp:
        first = dict(rule.FIRST_DRAW)
        rule.FIRST_DRAW["fintabnet"] = pathlib.Path(tmp) / "FINTABNET.json"
        # The draw below must land in the scratch directory. If the routing
        # regressed to MANIFESTS -- a set in both tables -- drawing here
        # would seal the repository instead of the fixture.
        routed = rule.draw_path_for("fintabnet")
        if routed.parent != pathlib.Path(tmp):
            rule.FIRST_DRAW.clear()
            rule.FIRST_DRAW.update(first)
            return [f"draw_path_for(fintabnet) is {routed}, outside the "
                    "scratch directory -- refusing to draw into the "
                    "repository."]
        try:
            try:
                manifest = bench.draw("fintabnet", entry, pages)
            except SystemExit as exc:
                return [f"draw(\"fintabnet\", ...) refused 10 synthetic pages: "
                        f"{exc} -- round 5's FATAL, reproduced off the rig."]
            written = rule.FIRST_DRAW["fintabnet"]
            if not written.is_file():
                failures.append("draw() returned a manifest but wrote no file "
                                "-- the round reads back what was written, "
                                "not what was returned.")
                return failures
            if json.loads(written.read_text()) != manifest:
                failures.append("the file draw() wrote is not the manifest it "
                                "returned.")
            holdout = [p for p in manifest["pages"] if p["split"] == "holdout"]
            tuning = [p for p in manifest["pages"] if p["split"] == "tuning"]
            if not holdout or not tuning:
                failures.append(f"the draw left a half empty "
                                f"({len(tuning)} tuning, {len(holdout)} "
                                "holdout) -- an empty holdout passes every "
                                "check below for the wrong reason.")
                return failures
            if manifest["seed"] != rule.seed_for("fintabnet"):
                failures.append(f"the draw seals under {manifest['seed']!r}, "
                                f"not the fintabnet seed "
                                f"{rule.seed_for('fintabnet')!r} -- a draw "
                                "under another set's seed ties this split's "
                                "order to that set's.")
            if manifest["seal"]["holdout"] != rule.digest_of(holdout):
                failures.append("the holdout seal is not the digest of the "
                                "holdout -- a seal over anything else seals "
                                "nothing.")
            if manifest["seal"]["tuning"] != rule.digest_of(tuning):
                failures.append("the tuning seal is not the digest of the "
                                "tuning half.")
            _want_tuning, want_holdout = rule.split(
                pages, seed=rule.seed_for("fintabnet"))
            if ({p["name"] for p in holdout}
                    != {p["name"] for p in want_holdout}):
                failures.append("the recorded split is not the one the "
                                "fintabnet seed produces.")
            if (bench._drawn_on("fintabnet", manifest["seal"]["holdout"])
                    != manifest["drawn"]):
                failures.append("_drawn_on re-dated a file with the same seal "
                                "-- the day it was drawn is not the day it "
                                "was redrawn.")
            if (bench._drawn_on("fintabnet", "0" * 64)
                    != dt.date.today().isoformat()):
                failures.append("_drawn_on kept the date of a file whose seal "
                                "moved -- a manifest whose pages moved is a "
                                "new draw and says so.")
        finally:
            rule.FIRST_DRAW.clear()
            rule.FIRST_DRAW.update(first)
    return failures


def first_draw_holds_no_committed_manifest() -> list[str]:
    """No set is in both tables, and no FIRST_DRAW path exists in the repo.

    A FIRST_DRAW path that exists is an unchecked manifest: the guard never
    reads it. This is what forces the follow-up commit -- the one that lands
    FINTABNET.json -- to move the row and write the ledger row with it.
    """
    failures: list[str] = []
    both = sorted(set(rule.MANIFESTS) & set(rule.FIRST_DRAW))
    if both:
        failures.append(f"{', '.join(both)} is in MANIFESTS and in FIRST_DRAW "
                        "-- a set is sealed or being sealed, never both: "
                        "move it to MANIFESTS and add the ledger row, or back "
                        "to FIRST_DRAW.")
    for name, path in rule.FIRST_DRAW.items():
        if path.is_file():
            failures.append(f"{path.relative_to(REPO)} exists while `{name}` "
                            "is still in FIRST_DRAW: an unchecked manifest "
                            "the guard never reads -- move it to MANIFESTS "
                            "and add the ledger row.")
    return failures


def the_guard_is_unchanged() -> list[str]:
    """The guard still exits 0 on this tree -- FIRST_DRAW must not move it."""
    done = subprocess.run([sys.executable, str(GUARD)], capture_output=True,
                          text=True, check=False, cwd=REPO)
    if done.returncode != 0:
        return [f"the guard exited {done.returncode} on a tree whose sealed "
                f"sets are drawn -- FIRST_DRAW must not move it off MANIFESTS, "
                f"and 3 is the announced not-a-pass, never a green:\n"
                f"{done.stdout}{done.stderr}"]
    return []


def the_lane_self_test_draws() -> list[str]:
    """The fetcher self-test draws fintabnet, and is green doing it."""
    failures: list[str] = []
    body = BENCH.read_text().split("def _self_test(", 1)[1].split(
        "\ndef main(", 1)[0]
    if not re.search(r"draw\(\s*[\"']fintabnet[\"']", body):
        failures.append("the fetcher self-test never draws fintabnet -- round "
                        "5's lane went red on a paid machine for exactly "
                        "this, and a lane that cannot seal must go red here.")
    done = subprocess.run([sys.executable, str(BENCH), "--self-test"],
                          capture_output=True, text=True, check=False,
                          cwd=REPO)
    if done.returncode != 0:
        failures.append(f"structured_bench.py --self-test is red:\n"
                        f"{done.stdout}{done.stderr}")
    return failures


def the_measure_reads_the_first_draw() -> list[str]:
    """`main()` with `--measure` reads the manifest through `draw_path_for`.

    The draw seals through `draw_path_for`, but the round below it read
    through `manifest_for` -- which only knows sealed sets -- so the
    FinTabNet round would seal its first draw and die measuring it. The
    five cases above all stay green under that routing, because none of
    them measures. This one runs `main()` with `--measure` on a fixture
    manifest under a temporary FIRST_DRAW path and tiny pages, with the
    budget, the tools and the run stubbed out: the only thing under test
    is which lookup finds the manifest.
    """
    pages = synthetic_pages(10)
    _tuning, holdout = rule.split(pages, seed=rule.seed_for("fintabnet"))
    held = {p["name"] for p in holdout}
    if not held or len(held) == len(pages):
        return ["the fixture split left a half empty -- an empty holdout "
                "would pass the routing check below for the wrong reason."]
    manifest = {"pages": [
        {"name": p["name"], "split": "holdout" if p["name"] in held else "tuning"}
        for p in pages]}
    want = sorted(p["name"] for p in pages if p["name"] not in held)
    with tempfile.TemporaryDirectory(prefix="structured-first-measure-") as tmp:
        tmpdir = pathlib.Path(tmp)
        work = tmpdir / "work"
        work.mkdir()
        (work / "pages.json").write_text(json.dumps(pages))
        drawn = tmpdir / "FINTABNET.json"
        drawn.write_text(json.dumps(manifest))
        first = dict(rule.FIRST_DRAW)
        rule.FIRST_DRAW["fintabnet"] = drawn
        # The measure below must read the scratch manifest. If the routing
        # regressed to MANIFESTS -- a set in both tables -- it would read
        # the repository instead of the fixture.
        if rule.draw_path_for("fintabnet").parent != tmpdir:
            rule.FIRST_DRAW.clear()
            rule.FIRST_DRAW.update(first)
            return [f"draw_path_for(fintabnet) is "
                    f"{rule.draw_path_for('fintabnet')}, outside the scratch "
                    "directory -- refusing to measure against the repository."]
        seen: dict = {}
        real_budget = bench.check_budget
        real_allowed = bench.tools_allowed
        real_measure = bench.measure
        real_write = bench.write_run
        real_table = bench.table_of
        real_argv = sys.argv

        def fake_measure(set_name: str, entry: dict, chosen: list[dict],
                         allowed: dict, refused: list[dict], binaries: dict,
                         run_id: str, note: str | None,
                         text_dir=None, geometry: str = "tail") -> dict:
            seen["set"] = set_name
            seen["pages"] = sorted(p["name"] for p in chosen)
            return {"run_id": run_id, "set": set_name}

        try:
            bench.check_budget = lambda force, set_name="": None  # noqa: E731
            bench.tools_allowed = lambda asked, register=None, meta=None: (  # noqa: E731
                {"pdfluent": {"name": "pdfluent"}}, [])
            bench.measure = fake_measure  # type: ignore[assignment]
            bench.write_run = lambda run: REPO / f"{run['run_id']}.json"  # noqa: E731
            bench.table_of = lambda run: "TABLE"  # noqa: E731
            sys.argv = ["structured_bench.py", "--set", "fintabnet",
                        "--work", str(work), "--measure", "--tools", "ours"]
            try:
                with contextlib.redirect_stdout(io.StringIO()), \
                        contextlib.redirect_stderr(io.StringIO()):
                    code = bench.main()
            except SystemExit as exc:
                return [f"main() --measure on the first draw died: {exc} -- "
                        "the round reads the manifest it has just sealed, "
                        "through draw_path_for, and a lookup that only knows "
                        "sealed sets dies exactly here."]
            if code != 0:
                return [f"main() --measure on the first draw exited {code} "
                        "-- the round reads the manifest it has just sealed, "
                        "through draw_path_for."]
            if seen.get("set") != "fintabnet":
                return ["main() --measure never reached the tools -- the "
                        "manifest lookup answered before the round did."]
            if seen.get("pages") != want:
                return [f"main() --measure read {seen.get('pages')}, not the "
                        f"tuning half {want} -- the round measures the draw "
                        "it has just sealed, through draw_path_for."]
        finally:
            rule.FIRST_DRAW.clear()
            rule.FIRST_DRAW.update(first)
            bench.check_budget = real_budget
            bench.tools_allowed = real_allowed
            bench.measure = real_measure
            bench.write_run = real_write
            bench.table_of = real_table
            sys.argv = real_argv
    return []


def main() -> int:
    failures: list[str] = []
    failures += draw_path_is_the_drawn_path()
    failures += the_bootstrap_seals()
    failures += first_draw_holds_no_committed_manifest()
    failures += the_guard_is_unchanged()
    failures += the_lane_self_test_draws()
    failures += the_measure_reads_the_first_draw()
    for line in failures:
        print(f"FAIL {line}")
    if failures:
        print(f"\n{len(failures)} of the first draw's guarantees do not hold.")
        return 1
    print("✓ the first draw: fintabnet seals on the rig under its own seed, "
          "the guard still reads only sealed sets, and the lane's self-test "
          "draws")
    return 0


if __name__ == "__main__":
    sys.exit(main())
