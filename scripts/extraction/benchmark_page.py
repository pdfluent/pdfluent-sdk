#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The public benchmark page, from the rig's registered runs (#631).

    benchmark_page.py                  write the page (a draft while anything is missing)
    benchmark_page.py --check          exit 1 unless the page could ship as it is
    benchmark_page.py --self-test      the generator on fixtures, in a second

ONE SOURCE, THE RUN FILES
=========================
Every number on the page is read from a run file in benchmarks/runs/structured/
and nowhere else: not the competitor cache (a round already carries every
cached column, scored by that round's scorer and with the dimensions of the run
that measured it -- T6, #1982), not PARITY.toml's pins (a reference is a bar,
not a result), and nothing typed. Per set the page shows master's newest daily
round, which is `structured_parity`'s own reading of the line: no `branch`,
and a round rescored since is read at its newest scoring, because a rescore
is the same measurement read with the corrected instrument.

WHAT SHIPS AND WHAT DOES NOT
============================
The page is written either way, so that it can be read and reviewed; what
`--check` refuses is SHIPPING it (the ticket's rule, owner's go on top):

  - a number that is not there: an axis the reference pins, a clock, a memory
    figure, a licence or an offline answer that the round left empty;
  - a set whose newest round is older than the newest round of any set -- one
    daily round measures every set, and a page that mixes days mixes trees;
  - a round with no claim ID in PAGE.toml, because a number that goes out
    carries an ID that points at a run (CLAIMS.md);
  - a round or a column the register marks `publishable: false`: the set's
    licence or the tool's says its numbers may be measured and not shown, and
    a page is showing them (codex, #1990).

A quality cell carries its denominator, `0.612 ⟨48⟩`: the units the mean is
over, which differ between columns when one emits fewer matchable blocks.

A draft says at the top that it is one and lists every refusal, so a draft
cannot be mistaken for the page.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
import tempfile
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "ci" / "loop"))
sys.path.insert(0, str(REPO / "scripts" / "extraction"))

import structured_parity as parity  # noqa: E402

PREFIX = "benchmark-page"
CONFIG = REPO / "benchmarks" / "structured" / "PAGE.toml"
RUNS = parity.RUNS
OURS = parity.OURS
LABELS = dict(parity.AXES)

# The columns beyond quality, from each column's `dimensions` (#621). The
# ticket names these five; the rest of the dimensions stay in the run file.
DIMENSIONS = (
    ("ms_p50", "ms/page p50"),
    ("ms_p95", "ms/page p95"),
    ("peak_rss_mb", "peak RSS MB"),
    ("licence", "licence"),
    ("offline", "offline"),
)

# How our clock is taken, until the rig measures otherwise (#631).
OUR_CLOCK = "single thread, one process per page, start-up included"


def load_config(path: pathlib.Path = CONFIG) -> dict:
    """The page's own settings: which sets, in which order, and the claim IDs."""
    loaded = tomllib.loads(path.read_text())
    loaded.setdefault("claims", {})
    if not loaded.get("sets"):
        raise SystemExit(f"[{PREFIX}] FATAL: {path.name} names no set, so the page "
                         "would be empty and say nothing about why.")
    return loaded


def newest_round(set_name: str, runs: pathlib.Path) -> tuple[pathlib.Path, dict] | None:
    """Master's newest round of one set, at its newest scoring, or None."""
    on_line = [p for p in parity.committed_runs(set_name, runs) if parity.on_masters_line(p)]
    if not on_line:
        return None
    path = on_line[-1]
    return path, json.loads(path.read_text())


def measured_when(run: dict) -> str:
    """The UTC stamp of the round that ran the tools, not of a later rescore."""
    return (run.get("rescored_from") or run["run_id"]).split("-")[-1]


def _number(value, places: int = 3) -> str:
    return f"{value:.{places}f}" if isinstance(value, (int, float)) else "—"


def _declared(value) -> str:
    if isinstance(value, bool):
        return "yes" if value else "no"
    return "—" if value in (None, "") else str(value)


def _where(tool: dict, run: dict) -> str:
    """Where a column's clock was taken: a cached column's was a fill run's machine."""
    measured = ((tool.get("from_cache") or {}).get("measured") or {})
    machine = measured.get("machine") or run.get("machine") or {}
    if isinstance(machine, dict):
        machine = machine.get("instance") or machine.get("platform") or ""
    return str(machine) or "—"


def structured_table(set_name: str, run: dict, axes: list[str], claim: str | None,
                     missing: list[str]) -> list[str]:
    """One set's table: quality per axis, then speed, memory, licence and offline."""
    head = [LABELS.get(a, a) for a in axes] + [label for _n, label in DIMENSIONS]
    lines = ["| tool | " + " | ".join(head) + " | OCR | measured on |",
             "|---|" + "---|" * (len(head) + 2)]
    for key, tool in run["tools"].items():
        name = tool.get("name", key)
        cells = []
        for axis in axes:
            value, units = parity.mean_of(tool, axis)
            if value is None:
                missing.append(f"{set_name}: {name} has no {axis}")
            # WITH ITS DENOMINATOR (codex, #1990). A column that emits fewer
            # matchable blocks is scored on fewer units, and a mean over 17 pages
            # beside one over 157 reads as comparable when it is not.
            cells.append(_number(value) + (f" ⟨{units}⟩" if value is not None else ""))
        dims = tool.get("dimensions") or {}
        for field, _label in DIMENSIONS:
            value = dims.get(field)
            if value is None:
                missing.append(f"{set_name}: {name} has no {field}")
            cells.append(_number(value, 0) if field.startswith(("ms_", "peak_"))
                         else _declared(value))
        ocr = str(dims.get("ocr") or "—")
        lines.append(f"| {name} | " + " | ".join(cells) + f" | {ocr} | {_where(tool, run)} |")
    for refusal in run.get("refused") or []:
        lines.append(f"| {refusal.get('tool')} | not measured: {refusal.get('why')} |")
    lines += ["", _source(run, claim)]
    return lines


def parsebench_table(run: dict, claim: str | None, missing: list[str]) -> list[str]:
    """ParseBench: its own scorer's headline per category; no clock of its own."""
    import parsebench_round as pb

    lines = ["| tool | " + " | ".join(f"{g} ({pb.HEADLINE[g]})" for g in pb.GROUPS)
             + " | pages answered | OCR |",
             "|---|" + "---|" * (len(pb.GROUPS) + 2)]
    for key, tool in run["tools"].items():
        name = tool.get("name", key)
        cells = []
        for group in pb.GROUPS:
            value = pb.headline_of(group, (tool.get("groups") or {}).get(group, {}))
            if value is None:
                missing.append(f"parsebench: {name} has no {group}")
            cells.append(_number(value))
        answered = tool.get("markdown") or {}
        lines.append(f"| {name} | " + " | ".join(cells)
                     + f" | {answered.get('answered', '—')}/{answered.get('asked', '—')}"
                     + f" | {tool.get('ocr') or '—'} |")
    for refusal in run.get("refused") or []:
        lines.append(f"| {refusal.get('tool')} | not measured: {refusal.get('why')} |")
    for what, why in (run.get("not_measured") or {}).items():
        lines += ["", f"Not measured: {what} -- {why}."]
    lines += ["", "Speed and memory are the DocLayNet and PubTables rounds' own; "
                  "ParseBench keeps scores only.", "", _source(run, claim)]
    return lines


def _source(run: dict, claim: str | None) -> str:
    rescored = f", rescored as `{run['run_id']}`" if run.get("rescored_from") else ""
    return (f"Source: claim {claim or '—'}, run `{run.get('rescored_from') or run['run_id']}`"
            f"{rescored}, {run.get('pages_measured', '—')} pages, half "
            f"`{run.get('half', '—')}`.")


def build(config: dict, runs: pathlib.Path = RUNS,
          parity_file: pathlib.Path = parity.PARITY) -> tuple[str, list[str]]:
    """The page, and every reason it may not ship yet."""
    pins = parity.load_reference(parity_file)["sets"] if parity_file.is_file() else {}
    refusals: list[str] = []
    sections: list[str] = []
    rounds: dict[str, dict] = {}
    for set_name, entry in config["sets"].items():
        found = newest_round(set_name, runs)
        title = entry.get("title", set_name)
        if found is None:
            refusals.append(f"{set_name}: no round of this set on master's line")
            sections += [f"## {title}", "", "Not measured yet.", ""]
            continue
        _path, run = found
        rounds[set_name] = run
        if run.get("publishable") is False:
            refusals.append(f"{set_name}: run {run['run_id']} is marked not publishable")
        for key, tool in (run.get("tools") or {}).items():
            if tool.get("publishable") is False:
                refusals.append(f"{set_name}: column {tool.get('name', key)} is marked "
                                "not publishable")
        claim = config["claims"].get(run["run_id"]) or config["claims"].get(
            run.get("rescored_from") or "")
        if not claim:
            refusals.append(f"{set_name}: run {run['run_id']} has no claim ID in PAGE.toml")
        missing: list[str] = []
        sections += [f"## {title}", ""]
        if entry.get("about"):
            sections += [entry["about"].strip(), ""]
        if set_name == "parsebench":
            sections += parsebench_table(run, claim, missing)
        else:
            axes = entry.get("axes") or list((pins.get(set_name) or {}).get("axes") or {})
            sections += structured_table(set_name, run, axes, claim, missing)
        refusals += missing
        sections.append("")
    stamps = {name: measured_when(run) for name, run in rounds.items()}
    if stamps:
        newest_day = max(stamps.values())[:8]
        for name, stamp in sorted(stamps.items()):
            if stamp[:8] < newest_day:
                refusals.append(f"{name}: its newest round ({stamp}) is older than the "
                                f"newest daily round ({newest_day})")
    head = [f"# {config.get('title', 'Structured extraction benchmark')}", ""]
    if refusals:
        head += ["> **DRAFT -- not for publication.** It cannot ship while:", ""]
        head += [f"> - {why}" for why in refusals] + [""]
    if config.get("intro"):
        head += [config["intro"].strip(), ""]
    head += [f"Our clock: {OUR_CLOCK}. A competitor's clock is the one taken when the "
             "rig last ran it, on the machine its row names.", ""]
    return "\n".join(head + sections).rstrip() + "\n", refusals


# ---------------------------------------------------------------------------
# The self-test: the generator on fixture runs, in a temporary directory.
# ---------------------------------------------------------------------------
def _fixture_run(set_name: str, stamp: str, *, branch: str | None = None,
                 dims: dict | None = None, text: float | None = 0.6,
                 refused: list | None = None) -> dict:
    full = {"ms_p50": 21.0, "ms_p95": 80.0, "peak_rss_mb": 120.0,
            "licence": "AGPL-3.0 or commercial", "offline": True, "ocr": "off"}
    column = {"name": "PDFluent", "summary": {"order": {"mean": 0.8, "pages": 10}},
              "in_documents": {"summary": {"text": {"mean": text, "pages": 3}}},
              "dimensions": dict(full, **(dims or {}))}
    other = {"name": "Docling", "summary": {"order": {"mean": 0.7, "pages": 10}},
             "in_documents": {"summary": {"text": {"mean": 0.65, "pages": 3}}},
             "dimensions": dict(full, licence="MIT", ms_p50=900.0),
             "from_cache": {"file": "x", "measured": {"run_id": "fill", "machine": "cpx42-fill"}}}
    run = {"run_id": f"structured-{set_name}-{stamp}", "set": set_name, "half": "tuning",
           "pages_measured": 10, "machine": {"instance": "cpx42-round"},
           "line": {"merge_base": "abc"} if branch is None else
                   {"branch": branch, "merge_base": "abc"},
           "tools": {"pdfluent": column, "docling": other}, "refused": refused or []}
    return run


def self_test() -> int:
    failures: list[str] = []

    def check(name: str, ok: bool, why: str) -> None:
        if not ok:
            failures.append(f"{name} -- {why}")

    with tempfile.TemporaryDirectory() as tmp:
        runs = pathlib.Path(tmp)
        config = {"title": "T", "sets": {"doclaynet": {"axes": ["text", "order"]}},
                  "claims": {"structured-doclaynet-20260926T010000Z": "C30"}}

        def write(run: dict) -> None:
            (runs / f"{run['run_id']}.json").write_text(json.dumps(run))

        none = runs / "none.toml"
        write(_fixture_run("doclaynet", "20260925T010000Z"))
        write(_fixture_run("doclaynet", "20260926T010000Z"))
        # A branch round, newer than both, is beside the line and not the page.
        write(_fixture_run("doclaynet", "20260927T010000Z", branch="t9/x", text=0.99))
        page, refusals = build(config, runs, none)
        check("the page reads master's newest round", "20260926T010000Z" in page
              and "0.990" not in page, page)
        check("every number carries its claim and its run",
              "claim C30, run `structured-doclaynet-20260926T010000Z`" in page, page)
        check("quality, clock, memory, licence and offline are on it",
              all(s in page for s in ("0.600", "21", "80", "120", "AGPL-3.0 or commercial",
                                      "yes", "MIT")), page)
        check("a cached column says where its clock was taken", "cpx42-fill" in page, page)
        check("our clock says how it is taken", OUR_CLOCK in page, page)
        check("a quality cell carries its denominator", "0.600 ⟨3⟩" in page, page)
        check("a complete, claimed, current page may ship", refusals == [], str(refusals))
        check("and is no draft", "DRAFT" not in page, page)

        # A number the round left out is a refusal, and the draft says which.
        write(_fixture_run("doclaynet", "20260928T010000Z", dims={"ms_p95": None},
                           text=None, refused=[{"tool": "marker", "why": "weights licence"}]))
        config["claims"]["structured-doclaynet-20260928T010000Z"] = "C31"
        page, refusals = build(config, runs, none)
        check("an empty clock refuses", any("ms_p95" in r for r in refusals), str(refusals))
        check("an empty axis refuses", any("has no text" in r for r in refusals), str(refusals))
        check("a draft says it is one", page.startswith("# T\n\n> **DRAFT"), page)
        check("a refused tool is shown with its reason",
              "| marker | not measured: weights licence |" in page, page)

        # The register's verdict: a run or a column that may not be shown.
        closed = _fixture_run("doclaynet", "20260928T010000Z")
        closed["publishable"] = False
        closed["tools"]["docling"]["publishable"] = False
        write(closed)
        _page, refusals = build(config, runs, none)
        check("a run marked not publishable refuses",
              any("is marked not publishable" in r and "run " in r for r in refusals),
              str(refusals))
        check("a column marked not publishable refuses",
              any("column Docling is marked not publishable" in r for r in refusals),
              str(refusals))
        write(_fixture_run("doclaynet", "20260928T010000Z", dims={"ms_p95": None},
                           text=None, refused=[{"tool": "marker", "why": "weights licence"}]))

        # No claim ID: refused.
        del config["claims"]["structured-doclaynet-20260928T010000Z"]
        _page, refusals = build(config, runs, none)
        check("a round with no claim ID refuses",
              any("no claim ID" in r for r in refusals), str(refusals))

        # Two sets from two days: the older one refuses.
        config["sets"]["pubtables-1m"] = {"axes": ["text"]}
        write(_fixture_run("pubtables-1m", "20260920T010000Z"))
        _page, refusals = build(config, runs, none)
        check("a set measured on an older day than the newest round refuses",
              any(r.startswith("pubtables-1m: its newest round") for r in refusals),
              str(refusals))

        # A rescore stands in for the round it re-read, and keeps its day.
        rescore = _fixture_run("pubtables-1m", "20260920T010000Z")
        rescore["run_id"] = "structured-pubtables-1m-20260920T010000Z-rescored-abc1234"
        rescore["rescored_from"] = "structured-pubtables-1m-20260920T010000Z"
        rescore["when"] = "2026-09-29T00:00:00Z"
        rescore["tools"]["pdfluent"]["in_documents"]["summary"]["text"]["mean"] = 0.444
        write(rescore)
        page, refusals = build(config, runs, none)
        check("a rescore is read in place of its round", "0.444" in page, page)
        check("and does not pass for a newer measurement",
              any(r.startswith("pubtables-1m: its newest round") for r in refusals),
              str(refusals))

        # A set with no round at all is said, not skipped.
        config["sets"]["parsebench"] = {"title": "ParseBench"}
        page, refusals = build(config, runs, none)
        check("a set with no round is refused and says so",
              "parsebench: no round of this set on master's line" in refusals
              and "## ParseBench\n\nNot measured yet." in page, page)

    if failures:
        for failure in failures:
            print(f"FAIL {failure}")
        return 1
    print(f"✓ {PREFIX}: the page reads master's newest round per set, carries a claim and "
          "a run id for every table, and refuses to ship an empty, unclaimed or stale number")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--out", default=str(REPO / "benchmarks" / "structured" / "PAGE.md"),
                        help="where to write the page (default: %(default)s)")
    parser.add_argument("--check", action="store_true",
                        help="write nothing; exit 1 unless the page could ship as it is")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    page, refusals = build(load_config())
    if args.check:
        for why in refusals:
            print(f"[{PREFIX}] cannot ship: {why}")
        if not refusals:
            print(f"[{PREFIX}] OK: every number is measured, claimed and from the newest round")
        return 1 if refusals else 0
    pathlib.Path(args.out).write_text(page)
    state = f"a DRAFT ({len(refusals)} reason(s) it cannot ship)" if refusals else "shippable"
    print(f"[{PREFIX}] wrote {args.out}: {state}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
