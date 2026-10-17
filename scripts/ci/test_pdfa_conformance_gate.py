#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The flavour gate judges each flavour against its own baseline (#443).

WHY A FAKE RUNNER AND A FAKE veraPDF

The gate's own logic -- which baseline it reads, which flavour it tells the
converter to produce, what it calls a regression -- is what can silently go
wrong, and none of it needs a real conversion to exercise. A test that needed
the 300-document corpus and a JVM would run nowhere, which is how a gate ends
up with no test at all.

So the runner here is a shell script that writes the results table the real one
writes, and it records the `--pdfa-level` it was handed. That last part is the
case that matters most: a gate that reads the 1b baseline while converting to
2b would be green on every push and wrong about every number.

Every path below is a temporary directory. The real
`benchmarks/pdfa/conformance_baseline*.json` files are never read and never
written -- a fixture that edits the baseline it is meant to be judging is the
accident `a_fixture_cannot_touch_the_real_ledger.py` exists for, one register
over.

Exit codes:
    0  every case passed
    1  a case failed
"""

from __future__ import annotations

import importlib.util
import io
import json
import os
import pathlib
import sys
import tempfile
from contextlib import redirect_stdout, redirect_stderr

HERE = pathlib.Path(__file__).resolve().parent

failures: list[str] = []
done = 0


def load(name: str, path: pathlib.Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


# Under its REAL module name, and before the gate is loaded. The gate does
# `from pdfa_flavours import baseline_path`, so a copy loaded under a test-only
# name would leave the gate holding a second, unpatched module -- which is not a
# theory: the first version of this file did exactly that and wrote four
# baselines into `benchmarks/pdfa/` while asserting it had not.
flavours = load("pdfa_flavours", HERE / "pdfa_flavours.py")
gate = load("pdfa_gate_under_test", HERE / "pdfa_conformance_gate.py")

REAL_BASELINE_DIR = HERE.parent.parent / "benchmarks" / "pdfa"
REAL_BASELINES_BEFORE = {
    p: p.read_bytes() for p in sorted(REAL_BASELINE_DIR.glob("conformance_baseline*.json"))
}

DOCS = [f"doc{i:02d}.pdf" for i in range(6)]

FAKE_RUNNER = """#!/usr/bin/env bash
# A stand-in for xfa-test-runner: writes the rows the gate reads, and records
# the flavour it was told to convert to.
set -euo pipefail
db=""; level=""; corpus=""
while [ $# -gt 0 ]; do
  case "$1" in
    --db) db="$2"; shift 2 ;;
    --pdfa-level) level="$2"; shift 2 ;;
    --corpus) corpus="$2"; shift 2 ;;
    *) shift ;;
  esac
done
echo "$level" >> "$RECORDED_LEVELS"
python3 - "$db" "$corpus" "$level" <<'PY'
import os, sqlite3, sys
db, corpus, level = sys.argv[1], sys.argv[2], sys.argv[3]
con = sqlite3.connect(db)
con.execute(
    "CREATE TABLE test_results (pdf_path TEXT, test_name TEXT, status TEXT)")
failing = set(os.environ.get("FAIL_AT_" + level, "").split()) - {""}
for name in sorted(os.listdir(corpus)):
    status = "fail" if name in failing else "pass"
    con.execute("INSERT INTO test_results VALUES (?,?,?)",
                (os.path.join(corpus, name), "pdfa_convert", status))
con.commit()
con.close()
PY
"""


def build_tree(tmp: pathlib.Path) -> tuple[pathlib.Path, pathlib.Path, pathlib.Path, pathlib.Path]:
    corpus = tmp / "corpus"
    corpus.mkdir()
    for name in DOCS:
        (corpus / name).write_bytes(b"%PDF-1.4\n")

    sample = tmp / "sample.txt"
    sample.write_text("\n".join(DOCS) + "\n")

    binary = tmp / "fake-runner"
    binary.write_text(FAKE_RUNNER)
    binary.chmod(0o755)

    verapdf = tmp / "fake-verapdf"
    verapdf.write_text("#!/usr/bin/env bash\necho 'veraPDF 1.28.2'\n")
    verapdf.chmod(0o755)
    return corpus, sample, binary, verapdf


def run_gate(argv: list[str]) -> tuple[int, str]:
    """Call the gate in this process and return (exit code, everything it said)."""
    out, err = io.StringIO(), io.StringIO()
    old = sys.argv
    sys.argv = ["pdfa_conformance_gate.py", *argv]
    code = 0
    try:
        with redirect_stdout(out), redirect_stderr(err):
            gate.main()
    except SystemExit as e:
        code = e.code if isinstance(e.code, int) else 1
    finally:
        sys.argv = old
    return code, out.getvalue() + err.getvalue()


def case(name: str, fn) -> None:
    global done
    done += 1
    try:
        fn()
    except AssertionError as e:
        failures.append(f"{name}: {e}")
    except Exception as e:  # noqa: BLE001 - a crashing case is a failing case
        failures.append(f"{name}: raised {type(e).__name__}: {e}")


def with_tree(fn):
    def wrapped():
        with tempfile.TemporaryDirectory(prefix="pdfa-gate-case-") as d:
            tmp = pathlib.Path(d)
            corpus, sample, binary, verapdf = build_tree(tmp)
            baselines = tmp / "baselines"
            baselines.mkdir()
            flavours.BASELINE_DIR = baselines
            os.environ["RECORDED_LEVELS"] = str(tmp / "levels.txt")
            try:
                fn(tmp, corpus, sample, binary, verapdf, baselines)
            finally:
                for key in list(os.environ):
                    if key.startswith("FAIL_AT_"):
                        del os.environ[key]
    return wrapped


def base_argv(corpus, sample, binary, verapdf, out) -> list[str]:
    return [
        "--corpus-dir", str(corpus),
        "--sample-list", str(sample),
        "--binary", str(binary),
        "--verapdf", str(verapdf),
        "--output-dir", str(out),
    ]


# --------------------------------------------------------------------------- #
# 1. A round of five flavours records five baselines, one per flavour, and
#    converts to each of them.
# --------------------------------------------------------------------------- #
@with_tree
def all_flavours_record(tmp, corpus, sample, binary, verapdf, baselines):
    code, said = run_gate(
        base_argv(corpus, sample, binary, verapdf, tmp / "out")
        + ["--all-flavours", "--update-baseline"]
    )
    assert code == 0, f"expected 0, got {code}: {said}"
    for name in flavours.FLAVOURS:
        path = flavours.FLAVOURS[name].baseline
        assert path.is_file(), f"no baseline written for {name}"
        doc = json.loads(path.read_text())
        entry = doc["platforms"][gate.platform_key()]
        assert entry["flavour"] == name, f"{name} baseline says {entry['flavour']}"
        assert entry["pass"] == len(DOCS), f"{name}: {entry['pass']} of {len(DOCS)}"
        assert len(entry["documents"]) == len(DOCS), f"{name} lost documents"
    levels = pathlib.Path(os.environ["RECORDED_LEVELS"]).read_text().split()
    assert sorted(levels) == sorted(flavours.FLAVOURS), f"converted to {levels}"


case("a round records one baseline per flavour", all_flavours_record)


# --------------------------------------------------------------------------- #
# 2. A failure at one flavour is red at that flavour and nowhere else. This is
#    the whole reason the baselines are separate files.
# --------------------------------------------------------------------------- #
@with_tree
def one_flavour_regresses(tmp, corpus, sample, binary, verapdf, baselines):
    run_gate(
        base_argv(corpus, sample, binary, verapdf, tmp / "out")
        + ["--all-flavours", "--update-baseline"]
    )
    os.environ["FAIL_AT_1b"] = DOCS[2]
    code, said = run_gate(
        base_argv(corpus, sample, binary, verapdf, tmp / "out2") + ["--all-flavours"]
    )
    assert code == 1, f"a regression at 1b must be red, got {code}: {said}"
    assert "1b:" in said, f"the reason must name 1b: {said}"
    assert DOCS[2] in said, f"the reason must name the document: {said}"
    for other in ("2b", "2u", "3b", "3u"):
        assert f"{other}: REGRESSION" not in said, f"{other} must not be blamed: {said}"
        assert f"{other}: 1 document" not in said, f"{other} must not be blamed: {said}"


case("a regression at one flavour is red at that flavour only", one_flavour_regresses)


# --------------------------------------------------------------------------- #
# 3. Dropping a document's verdict from a baseline is red: the run and the
#    baseline no longer describe the same set, and a count that happens to
#    match would hide that.
# --------------------------------------------------------------------------- #
@with_tree
def dropped_verdict(tmp, corpus, sample, binary, verapdf, baselines):
    run_gate(
        base_argv(corpus, sample, binary, verapdf, tmp / "out")
        + ["--all-flavours", "--update-baseline"]
    )
    path = flavours.FLAVOURS["3u"].baseline
    doc = json.loads(path.read_text())
    entry = doc["platforms"][gate.platform_key()]
    entry["documents"].pop(DOCS[1])
    path.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")

    code, said = run_gate(
        base_argv(corpus, sample, binary, verapdf, tmp / "out2") + ["--all-flavours"]
    )
    assert code == 1, f"a baseline missing a verdict must be red, got {code}: {said}"
    assert "3u:" in said, f"the reason must name 3u: {said}"


case("a baseline that lost a document's verdict is red", dropped_verdict)


# --------------------------------------------------------------------------- #
# 4. A baseline that predates #443 -- counts only, no per-document verdicts --
#    is still judged on its count rather than treated as broken.
# --------------------------------------------------------------------------- #
@with_tree
def old_shaped_baseline(tmp, corpus, sample, binary, verapdf, baselines):
    flavours.FLAVOURS["2b"].baseline.write_text(
        json.dumps(
            {
                "platforms": {
                    gate.platform_key(): {
                        "sample_size": len(DOCS),
                        "pass": len(DOCS),
                        "measured_total": len(DOCS),
                    }
                }
            },
            indent=2,
        )
        + "\n"
    )
    code, said = run_gate(base_argv(corpus, sample, binary, verapdf, tmp / "out"))
    assert code == 0, f"an older baseline must still be judged, got {code}: {said}"

    os.environ["FAIL_AT_2b"] = DOCS[0]
    code, said = run_gate(base_argv(corpus, sample, binary, verapdf, tmp / "out2"))
    assert code == 1, f"a drop against an older baseline must be red: {said}"


case("a baseline with counts only is still judged", old_shaped_baseline)


# --------------------------------------------------------------------------- #
# 5. `--baseline` names one file, so it cannot be combined with a round of
#    five. Without this the fifth flavour would overwrite the first four and
#    the file would be judged against the wrong flavour for ever after.
# --------------------------------------------------------------------------- #
@with_tree
def one_baseline_path_for_five(tmp, corpus, sample, binary, verapdf, baselines):
    code, said = run_gate(
        base_argv(corpus, sample, binary, verapdf, tmp / "out")
        + ["--all-flavours", "--baseline", str(tmp / "one.json")]
    )
    assert code == 2, f"expected a usage refusal, got {code}: {said}"


case("--baseline and --all-flavours are refused together", one_baseline_path_for_five)


# --------------------------------------------------------------------------- #
# 6. The default flavour is still 2b and still writes where it always wrote, so
#    the two CI jobs that name `measured.json` keep finding it.
# --------------------------------------------------------------------------- #
@with_tree
def default_is_2b(tmp, corpus, sample, binary, verapdf, baselines):
    out = tmp / "out"
    code, said = run_gate(base_argv(corpus, sample, binary, verapdf, out) + ["--update-baseline"])
    assert code == 0, said
    assert (out / "measured.json").is_file(), "measured.json moved"
    measured = json.loads((out / "measured.json").read_text())
    assert measured["flavour"] == "2b", measured["flavour"]
    assert flavours.FLAVOURS["2b"].baseline.name == "conformance_baseline.json"


case("the default flavour and its artefact path are unchanged", default_is_2b)


# The fixture's own alibi. Every case runs against a temporary baseline
# directory; if one of them ever reaches the committed baselines again, this
# says so here rather than in somebody's `git status` three commits later.
after = {
    p: p.read_bytes() for p in sorted(REAL_BASELINE_DIR.glob("conformance_baseline*.json"))
}
if after != REAL_BASELINES_BEFORE:
    changed = sorted(
        p.name for p in set(after) | set(REAL_BASELINES_BEFORE)
        if after.get(p) != REAL_BASELINES_BEFORE.get(p)
    )
    failures.append(
        "the fixture touched the committed baselines: " + ", ".join(changed)
    )
    done += 1

# --------------------------------------------------------------------------- #
# 7. A baseline recorded on another machine is not a verdict about this one, and
#    a round of five says so about all five rather than stopping at the first.
# --------------------------------------------------------------------------- #
@with_tree
def baseline_from_another_platform(tmp, corpus, sample, binary, verapdf, baselines):
    run_gate(
        base_argv(corpus, sample, binary, verapdf, tmp / "out")
        + ["--all-flavours", "--update-baseline"]
    )
    for name in flavours.FLAVOURS:
        path = flavours.FLAVOURS[name].baseline
        doc = json.loads(path.read_text())
        doc["platforms"] = {"someothermachine": doc["platforms"][gate.platform_key()]}
        path.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")

    code, said = run_gate(
        base_argv(corpus, sample, binary, verapdf, tmp / "out2") + ["--all-flavours"]
    )
    assert code == 1, f"expected 1, got {code}: {said}"
    for name in flavours.FLAVOURS:
        assert f"{name}: no baseline recorded" in said, f"{name} was not reported: {said}"


case("a baseline from another platform is reported for every flavour",
     baseline_from_another_platform)


# --------------------------------------------------------------------------- #
# 8. `--baseline` names a file, and a typo names somebody else's. A baseline
#    that says which flavour it holds is not overwritten by, or compared
#    against, a different one -- the 2u and 3u ledgers currently hold identical
#    verdicts, so nothing else would notice. (codex, #443)
# --------------------------------------------------------------------------- #
@with_tree
def baseline_belongs_to_another_flavour(tmp, corpus, sample, binary, verapdf, baselines):
    run_gate(
        base_argv(corpus, sample, binary, verapdf, tmp / "out")
        + ["--all-flavours", "--update-baseline"]
    )
    wrong = flavours.FLAVOURS["3u"].baseline

    code, said = run_gate(
        base_argv(corpus, sample, binary, verapdf, tmp / "out2")
        + ["--flavour", "2u", "--baseline", str(wrong), "--update-baseline"]
    )
    assert code == 1, f"overwriting 3u's ledger from a 2u run must be refused: {said}"
    assert "3u" in said and "2u" in said, said
    assert json.loads(wrong.read_text())["platforms"][gate.platform_key()]["flavour"] == "3u", \
        "the refused run wrote anyway"

    code, said = run_gate(
        base_argv(corpus, sample, binary, verapdf, tmp / "out3")
        + ["--flavour", "2u", "--baseline", str(wrong)]
    )
    assert code == 1, f"judging 2u against 3u's ledger must be refused: {said}"


case("a baseline that belongs to another flavour is refused",
     baseline_belongs_to_another_flavour)


# --------------------------------------------------------------------------- #
# 9. A licensed run and an unlicensed baseline converted different documents:
#    the free-tier watermark is a page the pipeline has to embed a font for. The
#    field was recorded from the start and never read back. (codex, #443)
# --------------------------------------------------------------------------- #
@with_tree
def licence_mode_must_match(tmp, corpus, sample, binary, verapdf, baselines):
    run_gate(
        base_argv(corpus, sample, binary, verapdf, tmp / "out")
        + ["--all-flavours", "--update-baseline"]
    )
    os.environ["PDFLUENT_LICENSE_KEY"] = "not-a-real-key-just-a-mode"
    try:
        code, said = run_gate(
            base_argv(corpus, sample, binary, verapdf, tmp / "out2") + ["--all-flavours"]
        )
    finally:
        del os.environ["PDFLUENT_LICENSE_KEY"]
    assert code == 1, f"a licensed run against an unlicensed baseline must be red: {said}"
    assert "unlicensed" in said and "licensed" in said, said
    for name in flavours.FLAVOURS:
        assert f"{name}: this run is licensed" in said, f"{name} was not reported: {said}"


case("a licensed run is not judged against an unlicensed baseline", licence_mode_must_match)


print(f"test_pdfa_conformance_gate: {done} case(s), {len(failures)} failure(s)")
for f in failures:
    print(f"  FAIL {f}")
sys.exit(1 if failures else 0)
