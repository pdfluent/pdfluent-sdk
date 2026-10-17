#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""One scorer for the header-band subset, base vs arm, the same way every time (#574).

The rule track closed several ParseBench/FinTabNet header and extent buckets
and handed their pages to the table models (tracks A/B) rather than fixing
them by rule. The subset (`header_band_subset.txt` beside this script, 28
pages) is those pages:

* m253 traced Home Depot p45's one chosen set becoming two emitted tables --
  the header band splitting off downstream of the stitch -- and counted the
  pages where one chosen set comes out as two or more `tableCell` runs whose
  first run holds no figure row (Home Depot x10, METLIFE p8/p16, JNPR
  2010/2011 on ParseBench tuning; 8 FinTabNet train-side pages).
* m267 traced the ParseBench "rows fused" losers the gap map named.
* m268 traced the header rows that come out unmarked (`<td>` for a real
  header: AZ LIC p105/p47, 203924 p12, blackrock-icav p25) and m270 counted
  its two narrow candidates (the rowspan veto in `header_block`, m267's
  cluster C4 on 203924 p21/p25). The rule track closed 04-10; these six
  pages are its remainder.

The tracks report on exactly these pages, base vs arm, through this one
script, so a number from track A and a number from track B are comparable.

WHAT IT MEASURES
  ParseBench pages (20): `table_record_match` + `grits_con` per page, through
  ParseBench's own scorer exactly as the `pbrun.py` tools (m158/m211) call
  it: the tree's `parsebench_harness` provider, `PipelineCLI().run` on the
  `table` group, process pools inlined where the sandbox denies POSIX
  semaphores. The per-page headline is the rig's (`grits_trm_composite`).
  FinTabNet pages (8, all train side): TEDS per truth table on the page.
  T1's scratch `ftn_engine.py` no longer exists on this machine, so this
  scores through the identical repo functions the CI round scores FinTabNet
  with: `fintabnet_archive` + `pubtables_cells.tables_by_page` for the truth
  HTML, `structured_bench.blocks_of` for the produced tables,
  `structured_axes.table_structure_score` for the pairing and the mean.

INVOCATION (in the venv that has parse-bench, with hash seed fixed so the
scorer's row order is stable -- the join is on test_id regardless):

    PYTHONHASHSEED=0 ~/.local/share/pdfluent/pb-venv/bin/python \\
        scripts/extraction/learned/subset_scorer.py \\
        --base <binary-or-md-dir> --arm <binary-or-md-dir> \\
        [--subset header_band_subset.txt] [--out subset_out]

(On a Mac whose shell runs under Rosetta, prefix `arch -arm64`: the venv's
native modules are arm64. Behind a TLS-intercepting proxy, also export
`SSL_CERT_FILE` at a bundle holding its CA, or the archive range reads
fail verification.)

A binary is run as `structured_markdown` (`[<binary> <pdf>]` with
`PDF_RENDER_THREADS=0`) on each subset page: ParseBench pages from
`~/.local/share/pdfluent/pb-tuning/data/docs/table/`, FinTabNet pages
fetched by HTTP range from the pinned archive exactly as the CI harness
fetches them (same register revisions, CRC32 + provenance + alignment
checks; the 8 subset companies are all train side, and an eval-side page
is refused -- the rig's pages are fetched on the round machine only).
A directory is read as existing markdown (`<stem>.pdf.md` / `<stem>.md`).

Output in --out: `scores.tsv` (page, set, base, arm, delta + the components
per side), `summary.txt` (per set mean base/arm over the pages both arms
scored, pages up/down/equal, the unanswered ids), `headers.txt` (the first
table's first 4 rows per page per arm, so a reader sees the header shape),
plus the `md/`, `cases/` and `pb/` trees the numbers came from.

`--self-test` runs base against itself and refuses unless every page is
scored and every delta is 0.
"""
from __future__ import annotations

import argparse
import concurrent.futures as cf
import contextlib
import hashlib
import io
import json
import os
import pathlib
import re
import subprocess
import sys
import tarfile
import time

REPO = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "scripts" / "extraction"))
sys.path.insert(0, str(REPO / "scripts" / "ci"))
sys.path.insert(0, str(REPO / "scripts" / "ci" / "loop"))

import fintabnet_archive as fa  # noqa: E402
import fintabnet_split  # noqa: E402
import pb_summary  # noqa: E402
import pubtables_cells  # noqa: E402
import structured_axes as axes  # noqa: E402
import structured_bench as sb  # noqa: E402
from parsebench_harness import register  # noqa: E402

SUBSET_FILE = pathlib.Path(__file__).resolve().parent / "header_band_subset.txt"
PB_DATA = pathlib.Path.home() / ".local" / "share" / "pdfluent" / "pb-tuning" / "data"
PB_TABLE_DIR = "docs/table"

RECORD = "table_record_match"
GRITS = "grits_con"
COMPOSITE = "grits_trm_composite"

FTN_STEM = re.compile(r"([A-Z]+)_(\d{4})_page_(\d+)")
TIMEOUT_S = 300


def load_subset(path: pathlib.Path) -> list[str]:
    """The subset's page stems, in file order; `#` comments and blanks skipped."""
    stems = [line.strip() for line in path.read_text(encoding="utf-8").splitlines()
             if line.strip() and not line.startswith("#")]
    if not stems:
        raise SystemExit(f"[subset] FATAL: {path} names no pages.")
    if len(set(stems)) != len(stems):
        raise SystemExit(f"[subset] FATAL: {path} names a page twice.")
    return stems


def classify(stem: str) -> str:
    """`fintabnet` for a FinTabNet page stem, else `parsebench`.

    A FinTabNet stem is `<COMPANY>_<YEAR>_page_<N>` with the company in plain
    capitals (`fintabnet_split`); no ParseBench tuning name has that shape
    (spaces, dots, lowercase, long hashes), so the match is exact.
    """
    return "fintabnet" if FTN_STEM.fullmatch(stem) else "parsebench"


def ftn_key(stem: str) -> tuple[str, str, int]:
    found = FTN_STEM.fullmatch(stem)
    if not found:
        raise SystemExit(f"[subset] FATAL: {stem!r} is not a FinTabNet page stem.")
    return found.group(1), found.group(2), int(found.group(3))


# ---------------------------------------------------------------------------
# FinTabNet pages: the pinned archive, as the CI harness fetches it.
# ---------------------------------------------------------------------------
def ftn_archive_source(entry: dict, revision: str) -> tuple[str, str]:
    """Where the archive is read from: the mirror, or the HF CDN as today.

    The same selection as `structured_bench._fintabnet_archive_source` (env
    var, then the file the round places, then the pinned HF URL): a subset
    run on the round machine reads the mirror the round read. The URL is
    never printed; callers name the label.
    """
    url = (os.environ.get(sb.FINTABNET_ARCHIVE_URL_ENV) or "").strip()
    if not url:
        try:
            url = sb.FINTABNET_ARCHIVE_URL_FILE.read_text().strip()
        except OSError:
            url = ""
    if url:
        return "gcs-mirror", url
    return "hf-cdn", fa.documents_url(
        entry["repo_id"], revision, entry["documents_archive"])


def ftn_page_text(path: pathlib.Path) -> str:
    """The page's text through poppler, normalised -- `sb._page_text`'s read."""
    out, _ms = sb.run_tool(["pdftotext", "-q", str(path), "-"], timeout=120)
    return axes.normalise(out or "").lower()


def ftn_aligned(tables: list[dict], page_text: str) -> bool:
    """Is this the page the annotation says it is? (`sb._aligned`'s rule.)"""
    import html as html_module

    wanted = [axes.normalise(t).lower()
              for table in tables
              for t in html_module.unescape(
                  " \0".join(re.findall(r">([^<]+)<", table["html"]))).split("\0")
              if len(axes.normalise(t)) >= sb.ALIGNED_MIN_CHARS]
    if not wanted:
        return False
    found = sum(1 for t in wanted if t in page_text)
    return found >= sb.ALIGNED_SHARE * len(wanted)


def fetch_ftn_pages(stems: list[str], cache: pathlib.Path) -> dict[str, list[str]]:
    """The subset's FinTabNet pages in `cache`, and their truth-table HTML.

    Returns stem -> truth HTML per table above the floor (page 0, in
    annotation order). Pages and annotations are cached under `cache` with
    the revisions they were read at, so a re-run is offline; a cache read
    at other revisions is re-fetched rather than trusted.
    """
    from huggingface_hub import hf_hub_download

    entry = sb.dataset_entry("fintabnet")
    revision = entry.get("revision")
    arevision = entry.get("annotations_revision")
    if not revision or not arevision:
        raise SystemExit("[subset] FATAL: the fintabnet register row pins no "
                         "`revision`/`annotations_revision`; refusing an unpinned fetch.")
    keys = {stem: ftn_key(stem) for stem in stems}
    for stem, (company, _year, _num) in keys.items():
        # THE RIG'S PAGES ARE NOT FETCHED HERE. The 8 subset companies are
        # all train side; an eval-side page in a custom subset is the rig's,
        # fetched on the round machine only (`rig_data_stays_on_the_rig`).
        if fintabnet_split.side(company) == "eval":
            raise SystemExit(
                f"[subset] FATAL: {stem} is an eval-side company: the rig's "
                "pages are fetched on the round machine only (#574).")
    pages_dir = cache / "pages"
    ann_dir = cache / "annotations"
    pages_dir.mkdir(parents=True, exist_ok=True)
    ann_dir.mkdir(parents=True, exist_ok=True)
    pins_path = cache / "pins.json"
    pins = {"repo_id": entry["repo_id"], "revision": revision,
            "annotations_repo_id": entry["annotations_repo_id"],
            "annotations_revision": arevision, "pages": {}}
    if pins_path.is_file():
        try:
            cached = json.loads(pins_path.read_text())
        except ValueError:
            cached = {}
        if all(cached.get(k) == pins[k] for k in
               ("repo_id", "revision", "annotations_repo_id", "annotations_revision")):
            pins = cached
    missing = [s for s in stems if s not in pins.get("pages", {})
               or not (pages_dir / f"{s}.pdf").is_file()
               or not (ann_dir / f"{s}.json").is_file()]
    if missing:
        print(f"[subset] fetching {entry['annotations_file']} at {arevision[:12]}",
              flush=True)
        annotations_tar = hf_hub_download(
            repo_id=entry["annotations_repo_id"], filename=entry["annotations_file"],
            repo_type="dataset", revision=arevision, cache_dir=str(cache / "hf"))
        with tarfile.open(annotations_tar, "r|gz") as tar:
            names = fa.annotation_names(tar)
        print(f"[subset] {len(names)} annotated page(s) in the archive", flush=True)
        want_keys = {keys[s] for s in missing}
        absent = sorted(fa.walk_name(*k) for k in want_keys if k not in names)
        if absent:
            raise SystemExit(f"[subset] FATAL: no annotation for "
                             f"{', '.join(absent)} in the pinned annotations.")
        wanted = {names[k] for k in want_keys}
        with tarfile.open(annotations_tar, "r|gz") as tar:
            annotations, unreadable = fa.read_annotations(tar, wanted)
        if unreadable:
            raise SystemExit(f"[subset] FATAL: {unreadable} annotation(s) unreadable.")
        source, url = ftn_archive_source(entry, revision)
        print(f"[subset] reading the central directory of {entry['documents_archive']} "
              f"by range from {'the GCS mirror' if source == 'gcs-mirror' else 'the HF CDN'}",
              flush=True)
        reader = fa.RangeReader(url)
        # The reader retries each range five times, but a throttled CDN
        # refuses whole minutes after a 255 MB transfer, not single reads
        # (`fintabnet_archive`: 429 under load is a queue, not an answer) --
        # so the directory open gets three attempts a minute apart before
        # the fetch fails loudly.
        archive = None
        for attempt in range(3):
            try:
                archive = fa.open_archive(reader.read, int(entry["documents_archive_bytes"]))
                break
            except SystemExit as exc:
                if attempt == 2 or "did not answer" not in str(exc):
                    raise
                print(f"[subset] the archive did not answer; retrying "
                      f"({attempt + 2}/3)", flush=True)
                time.sleep(60)
        assert archive is not None
        fa.check_set(archive, int(entry["documents_pdfs"]))
        expected_jsonl = {row.get("file"): row
                          for row in (entry.get("jsonl_files") or []) if row.get("file")}
        jsonls = fa.JsonlCache(archive, expected=expected_jsonl)
        for stem in missing:
            company, year, num = keys[stem]
            annotation = annotations.get((company, year, num))
            if annotation is None:
                raise SystemExit(f"[subset] FATAL: {stem}: its annotation was not "
                                 "read from the archive.")
            kept = fa.kept_tables(annotation)
            for table in kept:
                try:
                    fa.check_provenance(company, year, num, table, jsonls)
                except fa.ProvenanceRefused as exc:
                    raise SystemExit(f"[subset] FATAL: {stem}: provenance mismatch: "
                                     f"{exc}.") from None
            try:
                blob = archive.fetch(fa.zip_path_of(company, year, num))
            except (fa.CRCRefused, fa.MemberRefused, fa.TransportFailed) as exc:
                raise SystemExit(f"[subset] FATAL: {stem}: the page did not answer: "
                                 f"{exc}.") from None
            page = pages_dir / f"{stem}.pdf"
            page.write_bytes(blob)
            tables = pubtables_cells.tables_by_page(kept).get(0, [])
            if not tables:
                page.unlink(missing_ok=True)
                raise SystemExit(f"[subset] FATAL: {stem} names no table above the "
                                 "floor; nothing to grade it against.")
            if not ftn_aligned(tables, ftn_page_text(page)):
                page.unlink(missing_ok=True)
                raise SystemExit(f"[subset] FATAL: {stem}: the alignment check "
                                 "failed; refusing pages that differ.")
            (ann_dir / f"{stem}.json").write_text(json.dumps(kept))
            pins["pages"][stem] = hashlib.sha256(blob).hexdigest()
        pins_path.write_text(json.dumps(pins, indent=1, sort_keys=True) + "\n")
    truth: dict[str, list[str]] = {}
    for stem in stems:
        kept = json.loads((ann_dir / f"{stem}.json").read_text())
        tables = pubtables_cells.tables_by_page(kept).get(0, [])
        if not tables:
            raise SystemExit(f"[subset] FATAL: {stem} names no table above the floor.")
        truth[stem] = [t["html"] for t in tables]
    return truth


# ---------------------------------------------------------------------------
# Markdown per arm: a binary run, or a directory read.
# ---------------------------------------------------------------------------
def run_binary(binary: str, pdf: pathlib.Path) -> tuple[str | None, str]:
    """`structured_markdown` on one page, single-threaded; (markdown, reason)."""
    env = {**os.environ, "PDF_RENDER_THREADS": "0"}
    try:
        done = subprocess.run([binary, str(pdf)], capture_output=True, text=True,
                              timeout=TIMEOUT_S, env=env)
    except subprocess.TimeoutExpired:
        return None, f"timed out after {TIMEOUT_S} s"
    except OSError as exc:
        return None, f"did not run: {exc}"
    if done.returncode != 0:
        tail = (done.stderr or "").strip().splitlines()
        return None, f"exit {done.returncode}" + (f": {tail[-1]}" if tail else "")
    return done.stdout, ""


def read_md_dir(directory: pathlib.Path, stem: str) -> tuple[str | None, str]:
    """Existing markdown for one page: `<stem>.pdf.md` / `<stem>.md`, by layout."""
    candidates = [directory / f"{stem}.pdf.md", directory / f"{stem}.md",
                  directory / PB_TABLE_DIR / f"{stem}.pdf.md",
                  directory / PB_TABLE_DIR / f"{stem}.md"]
    for path in candidates:
        if path.is_file():
            return path.read_text(encoding="utf-8", errors="replace"), ""
    found = sorted(directory.rglob(f"{stem}.pdf.md")) + sorted(directory.rglob(f"{stem}.md"))
    if found:
        return found[0].read_text(encoding="utf-8", errors="replace"), ""
    return None, f"no {stem}.pdf.md or {stem}.md under {directory}"


def obtain_markdown(source: str, stems: list[str], kinds: dict[str, str],
                    pdf_of: dict[str, pathlib.Path], keep: pathlib.Path,
                    ) -> tuple[dict[str, str | None], dict[str, str]]:
    """Each page's markdown for one arm, kept under `keep` for the record.

    `pdf_of` maps every stem to its page PDF (ParseBench from the local
    tuning copy, FinTabNet from the fetched cache). Returns (markdown,
    reason): `reason[stem]` is empty when answered, else why not.
    """
    path = pathlib.Path(source)
    markdown: dict[str, str | None] = {}
    reason: dict[str, str] = {}
    if path.is_dir():
        for stem in stems:
            text, why = read_md_dir(path, stem)
            markdown[stem], reason[stem] = text, why
    elif path.is_file() and os.access(path, os.X_OK):
        for stem in stems:
            text, why = run_binary(source, pdf_of[stem])
            markdown[stem], reason[stem] = text, why
            print(f"[subset] {path.name} {stem}: "
                  f"{'answered' if text is not None else 'unanswered (' + why + ')'}",
                  flush=True)
    else:
        raise SystemExit(f"[subset] FATAL: {source} is neither an executable binary "
                         "nor a markdown directory.")
    for stem, text in markdown.items():
        if text is None:
            continue
        dest = keep / PB_TABLE_DIR / f"{stem}.pdf.md" if kinds[stem] == "parsebench" \
            else keep / "ftn" / f"{stem}.md"
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(text)
    return markdown, reason


# ---------------------------------------------------------------------------
# ParseBench pages: the harness, as pbrun.py calls it.
# ---------------------------------------------------------------------------
class InlinePool:
    """A `ProcessPoolExecutor` that runs inline (`pbrun.py`, m158/m211).

    The sandbox denies the POSIX semaphores multiprocessing needs, so the
    pools run in this process: same register + `PipelineCLI().run`
    arguments, same worker functions, same aggregation -- only concurrency
    differs.
    """

    def __init__(self, max_workers=None, *args, **kwargs):
        pass

    def submit(self, fn, *args, **kwargs):
        future = cf.Future()
        try:
            future.set_result(fn(*args, **kwargs))
        except BaseException as exc:
            future.set_exception(exc)
        return future

    def map(self, fn, *iterables, **kwargs):
        return [fn(*item) for item in zip(*iterables)]

    def shutdown(self, wait=True, cancel_futures=False):
        pass

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.shutdown()
        return False


def patch_pools_inline() -> None:
    """Run ParseBench's three process pools in this process (`pbrun.py`)."""
    try:
        import parse_bench.evaluation.evaluators.parse as parsers
        import parse_bench.evaluation.metrics.parse.grits_metric as grits
        import parse_bench.evaluation.runner as runner
    except ImportError as exc:
        raise SystemExit(
            "[subset] FATAL: parse-bench is not installed for this interpreter "
            f"({exc}). Run with the pb-venv python, e.g. "
            "`arch -arm64 ~/.local/share/pdfluent/pb-venv/bin/python`.") from None
    runner.ProcessPoolExecutor = InlinePool
    parsers.ProcessPoolExecutor = InlinePool
    grits.ProcessPoolExecutor = InlinePool


def build_cases(stems: list[str], cases: pathlib.Path) -> None:
    """ParseBench's directory layout, holding only the subset's pages.

    The same shape as `parsebench_round.build_cases`: symlinks under
    `docs/table/` plus the `table.jsonl` rules for exactly these pages.
    """
    import shutil

    if cases.exists():
        shutil.rmtree(cases)
    wanted = {f"{PB_TABLE_DIR}/{stem}.pdf" for stem in stems}
    for stem in stems:
        target = cases / PB_TABLE_DIR / f"{stem}.pdf"
        target.parent.mkdir(parents=True, exist_ok=True)
        os.symlink(PB_DATA / PB_TABLE_DIR / f"{stem}.pdf", target)
    rules = PB_DATA / "table.jsonl"
    if not rules.is_file():
        raise SystemExit(f"[subset] FATAL: {rules} is not there; the tuning copy "
                         "holds the rules beside the pages.")
    kept, seen = [], set()
    for line in rules.read_text(encoding="utf-8").splitlines():
        if line.strip() and json.loads(line).get("pdf") in wanted:
            kept.append(line)
            seen.add(json.loads(line)["pdf"])
    if seen != wanted:
        missing = sorted(wanted - seen)
        raise SystemExit(f"[subset] FATAL: ParseBench has no table rules for "
                         f"{', '.join(missing)}; nothing to grade them against.")
    (cases / "table.jsonl").write_text("\n".join(kept) + ("\n" if kept else ""))


def score_parsebench(tool: str, md: pathlib.Path, cases: pathlib.Path,
                     out: pathlib.Path) -> tuple[dict, dict, list[str]]:
    """One arm through ParseBench's own scorer; (report, guards, stall ids)."""
    import shutil

    from parse_bench.pipeline.cli import PipelineCLI

    if out.exists():
        shutil.rmtree(out)
    name = register(tool, str(md))
    log = io.StringIO()
    with contextlib.redirect_stdout(log), contextlib.redirect_stderr(log):
        PipelineCLI().run(name, input_dir=str(cases), output_dir=str(out),
                          max_concurrent=4, group="table", open_report=False)
    # With a single group the report sits directly under the pipeline dir
    # (the per-group level of `parsebench_round` is the unfiltered run's).
    report_path = out / name / "_evaluation_report.json"
    if not report_path.is_file():
        raise SystemExit(f"[subset] FATAL: the scorer wrote no report for {tool}; "
                         f"its log ends:\n{log.getvalue()[-2000:]}")
    report = json.loads(report_path.read_text())
    return report, pb_summary.scan_guards(report), pb_summary.stall_ids_in(log.getvalue())


def pb_scores(report: dict, stems: list[str],
              markdown: dict[str, str | None]) -> dict[str, dict]:
    """Per subset page: headline + record + GriTS, or unanswered.

    A page the arm did not answer is FAIL even though the pipeline scores
    its missing file as a 0.0 row: a zero would read as "attempted and
    wrong" and would drag the survivor mean, while the truth is the arm
    never answered (a track without a ParseBench adapter hands no PB
    markdown at all).
    """
    by_id = {row.get("test_id"): row for row in report.get("per_example_results") or []}
    out: dict[str, dict] = {}
    for stem in stems:
        if markdown.get(stem) is None:
            out[stem] = {"score": None, "detail": "unanswered", "success": False}
            continue
        row = by_id.get(f"table/{stem}")
        values = {m.get("metric_name"): m.get("value") for m in (row.get("metrics") or [])
                  if isinstance(m, dict)} if row else {}
        head = values.get(COMPOSITE)
        if row is None or not row.get("success") or not isinstance(head, (int, float)):
            out[stem] = {"score": None, "detail": "the scorer did not evaluate it",
                         "success": bool(row and row.get("success"))}
            continue
        rec, gri = values.get(RECORD), values.get(GRITS)
        out[stem] = {"score": float(head), "success": True,
                     "detail": f"record={rec:.6f} grits={gri:.6f}"
                     if isinstance(rec, (int, float)) and isinstance(gri, (int, float))
                     else f"record={rec} grits={gri}"}
    return out


# ---------------------------------------------------------------------------
# FinTabNet pages: TEDS per truth table.
# ---------------------------------------------------------------------------
def ftn_scores(markdown: dict[str, str | None], truth: dict[str, list[str]],
               stems: list[str]) -> dict[str, dict]:
    """Per subset page: mean TEDS over its truth tables + TEDS per table."""
    out: dict[str, dict] = {}
    for stem in stems:
        text = markdown.get(stem)
        if text is None:
            out[stem] = {"score": None, "detail": "unanswered"}
            continue
        produced = [b.get("html", "") for b in sb.blocks_of(text)
                    if b["kind"] == "table"]
        result = axes.table_structure_score(truth[stem], produced)
        per = " ".join(f"t{i}={v:.6f}" for i, v in enumerate(result["per_table"]))
        out[stem] = {"score": float(result["teds"]), "success": True,
                     "detail": f"{per} (truth {result['truth']}, "
                               f"produced {result['produced']}, matched {result['matched']})"}
    return out


# ---------------------------------------------------------------------------
# Output: the TSV, the summary, the header shapes.
# ---------------------------------------------------------------------------
def first_table_rows(markdown: str | None) -> list[str]:
    """The first table's first 4 rows, as emitted (pipe rows or HTML lines)."""
    if markdown is None:
        return ["(unanswered)"]
    text = sb.COMMENT.sub(" ", sb.IMAGE.sub(" ", markdown))
    rows: list[str] = []
    for line in sb.FENCE.sub("", text).splitlines():
        if sb.TABLE_ROW.match(line):
            if not sb.TABLE_RULE.match(line):
                rows.append(line.strip())
            if len(rows) == 4:
                return rows
        elif rows:
            return rows
    if rows:
        return rows
    inside = False
    html: list[str] = []
    for line in text.splitlines():
        if "<table" in line.lower():
            inside = True
        if inside:
            html.append(line.strip())
            if len(html) == 4 or "</table>" in line.lower():
                return html
    return ["(no table)"]


def fmt(value: float | None) -> str:
    return f"{value:.6f}" if isinstance(value, (int, float)) else "FAIL"


def write_outputs(out: pathlib.Path, stems: list[str], kinds: dict[str, str],
                  base: dict[str, dict], arm: dict[str, dict],
                  markdown: dict[str, dict[str, str | None]]) -> str:
    """`scores.tsv` + `summary.txt` + `headers.txt`; returns the summary."""
    rows = []
    for stem in stems:
        b, a = base[stem]["score"], arm[stem]["score"]
        if b is None and a is None:
            delta = "n/a (both unanswered)"
        elif b is None or a is None:
            delta = "n/a (base unanswered)" if b is None else "n/a (arm unanswered)"
        else:
            delta = f"{a - b:+.6f}"
        rows.append((stem, kinds[stem], fmt(b), fmt(a), delta,
                     base[stem]["detail"], arm[stem]["detail"]))
    tsv = ["page\tset\tbase\tarm\tdelta\tbase_detail\tarm_detail"]
    tsv += ["\t".join(r) for r in rows]
    (out / "scores.tsv").write_text("\n".join(tsv) + "\n")

    summary = []
    for name in ("parsebench", "fintabnet"):
        mine = [stem for stem in stems if kinds[stem] == name]
        if not mine:
            continue
        both = [s for s in mine if base[s]["score"] is not None
                and arm[s]["score"] is not None]
        up = sum(1 for s in both if arm[s]["score"] - base[s]["score"] > 1e-9)
        down = sum(1 for s in both if arm[s]["score"] - base[s]["score"] < -1e-9)
        summary.append(f"[{name}] scored {len(both)}/{len(mine)} by both arms")
        if both:
            mb = sum(base[s]["score"] for s in both) / len(both)
            ma = sum(arm[s]["score"] for s in both) / len(both)
            summary.append(f"  mean base {mb:.6f} arm {ma:.6f} delta {ma - mb:+.6f}; "
                           f"pages up {up} down {down} equal {len(both) - up - down}")
        for side, scores in (("base", base), ("arm", arm)):
            missing = [s for s in mine if scores[s]["score"] is None]
            if missing:
                summary.append(f"  {side} unanswered: {', '.join(missing)}")
    text = "\n".join(summary) + "\n"
    (out / "summary.txt").write_text(text)

    headers = ["The first table's first 4 rows per page per arm, as emitted.",
               "Pipe tables: the first 4 non-delimiter rows; HTML: the first 4 lines.",
               ""]
    for stem in stems:
        headers.append(f"== {stem} [{kinds[stem]}] ==")
        for side in ("base", "arm"):
            headers.append(f"-- {side} --")
            headers += first_table_rows(markdown[side].get(stem))
        headers.append("")
    (out / "headers.txt").write_text("\n".join(headers))
    return text


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--base", required=True, help="a structured_markdown binary or a "
                    "markdown directory (<stem>.pdf.md / <stem>.md)")
    ap.add_argument("--arm", default=None, help="the same for the other arm; "
                    "defaults to --base")
    ap.add_argument("--subset", default=str(SUBSET_FILE))
    ap.add_argument("--out", default="subset_out")
    ap.add_argument("--ftn-cache", default=None, help="where the FinTabNet pages "
                    "and annotations are fetched to; defaults to <out>/ftn_cache")
    ap.add_argument("--self-test", action="store_true", help="run base against "
                    "itself and refuse unless every page is scored and every "
                    "delta is 0")
    args = ap.parse_args()
    arm_source = args.arm or args.base
    if args.self_test:
        arm_source = args.base
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    ftn_cache = pathlib.Path(args.ftn_cache) if args.ftn_cache else out / "ftn_cache"

    stems = load_subset(pathlib.Path(args.subset))
    kinds = {stem: classify(stem) for stem in stems}
    pb_stems = [s for s in stems if kinds[s] == "parsebench"]
    ftn_stems = [s for s in stems if kinds[s] == "fintabnet"]
    print(f"[subset] {len(stems)} pages ({len(pb_stems)} parsebench, "
          f"{len(ftn_stems)} fintabnet)", flush=True)

    for stem in pb_stems:
        if not (PB_DATA / PB_TABLE_DIR / f"{stem}.pdf").is_file():
            raise SystemExit(f"[subset] FATAL: {PB_DATA / PB_TABLE_DIR / f'{stem}.pdf'} "
                             "is not there.")
    truth = fetch_ftn_pages(ftn_stems, ftn_cache) if ftn_stems else {}
    pdf_of = {s: PB_DATA / PB_TABLE_DIR / f"{s}.pdf" for s in pb_stems}
    pdf_of.update({s: ftn_cache / "pages" / f"{s}.pdf" for s in ftn_stems})

    markdown: dict[str, dict] = {}
    reasons: dict[str, dict] = {}
    for side, source in (("base", args.base), ("arm", arm_source)):
        markdown[side], reasons[side] = obtain_markdown(
            source, stems, kinds, pdf_of, out / "md" / side)

    base: dict[str, dict] = {}
    arm: dict[str, dict] = {}
    refusals: list[str] = []
    if pb_stems:
        patch_pools_inline()
        cases = out / "cases"
        build_cases(pb_stems, cases)
        for side, scores in (("base", base), ("arm", arm)):
            tool = f"hb_{side}"
            report, guards, stalls = score_parsebench(
                tool, out / "md" / side, cases, out / "pb" / side)
            # A page the arm did not answer fails its evaluation by design
            # (the harness: no file, no answer) -- that is the page's
            # verdict, a FAIL in the TSV, not a refusal of the run. Only a
            # page the arm DID answer may refuse it.
            answered = {f"table/{s}" for s in pb_stems
                        if markdown[side][s] is not None}
            for test_id in guards["rule_timeout_docs"]:
                refusals.append(f"{tool}/{test_id}: rule_timeout")
            for test_id in guards["over_budget_docs"]:
                refusals.append(f"{tool}/{test_id}: over_budget")
            for test_id in guards["normalize_docs"]:
                refusals.append(f"{tool}/{test_id}: normalize_timeout")
            for test_id in guards["failed_docs"]:
                if test_id in answered:
                    refusals.append(f"{tool}/{test_id}: evaluation_failed")
            for test_id in stalls:
                refusals.append(f"{tool}/{test_id}: worker_timeout")
            scores.update(pb_scores(report, pb_stems, markdown[side]))
            for stem in pb_stems:
                if scores[stem]["score"] is None and markdown[side][stem] is not None:
                    print(f"[subset] {tool} {stem}: answered but not evaluated",
                          flush=True)
    if ftn_stems:
        base.update(ftn_scores(markdown["base"], truth, ftn_stems))
        arm.update(ftn_scores(markdown["arm"], truth, ftn_stems))
    for stem in stems:
        for side, scores in (("base", base), ("arm", arm)):
            if scores[stem]["score"] is None and markdown[side][stem] is None \
                    and reasons[side][stem]:
                scores[stem]["detail"] = f"unanswered ({reasons[side][stem]})"

    text = write_outputs(out, stems, kinds, base, arm, markdown)
    print(text, end="")
    print(f"[subset] base: {args.base}\n[subset] arm: {arm_source}", flush=True)
    if refusals:
        # Like the round: a timeout or a failed evaluation fails loudly and
        # never prints as a number -- a genuine failure is zero-padded into
        # the scorer's own average and would move the headline.
        for refusal in sorted(refusals):
            print(f"[subset] FAIL: {refusal}", flush=True)
        return 1
    failed = [s for s in stems
              if base[s]["score"] is None or arm[s]["score"] is None]
    if args.self_test:
        nonzero = [s for s in stems if base[s]["score"] is not None
                   and arm[s]["score"] is not None
                   and abs(arm[s]["score"] - base[s]["score"]) > 1e-9]
        if failed or nonzero:
            for stem in failed:
                print(f"[subset] SELF-TEST FAIL: {stem} not scored by both arms",
                      flush=True)
            for stem in nonzero:
                print(f"[subset] SELF-TEST FAIL: {stem} delta "
                      f"{arm[stem]['score'] - base[stem]['score']:+.6f}", flush=True)
            return 1
        print(f"[subset] SELF-TEST OK: {len(stems)} pages, all deltas 0", flush=True)
        return 0
    if failed and all(base[s]["score"] is None or arm[s]["score"] is None
                      for s in stems):
        print("[subset] FAIL: no page scored by both arms", flush=True)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
