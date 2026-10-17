#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Pages of PubTables' TRAINING split, to measure our word boxes against theirs (#600).

The train/serve gap is the difference between the boxes a model was trained on
-- PubTables' own -- and the boxes it will be given at reading time, which are
ours. Measuring it needs our engine to run over the same pages, and our engine
needs the pages.

THE RIG STAYS OFF THIS MACHINE, both halves of it (owner, 23-09-2026). So the
pages here come from documents the rig does not hold at all, and the comparison
is the same comparison: their boxes against ours, on the same tables, through
the same renderer and the same scorer.

EVERY FETCH IS THROUGH THE RIG'S OWN PATH and not a second one of ours:
`article_record`, `may_fetch`, `_get`, `merge_document` and `_aligned` are
`structured_bench.py`'s, so the licence code is read BEFORE the bytes and only
CC0 and CC BY are kept -- the verdict in DATASETS.toml, enforced by the code
that already enforces it everywhere else.

NAMED DOCUMENTS (#638). `--documents FILE` fetches only the documents the file
names -- one PMC id per line, `#` comments -- instead of the first N in
tarball order, which is how one era ended up on each side of the split.
`--pages-per-document` caps the table pages per document. The free-disk floor
is read on `--out`'s filesystem, so the script runs on Linux too.

ONE SPLIT PER RUN. `--split train` (the default) keeps the training split,
for the training draws; `--split val` keeps the validation split, for the
dev fetches -- the PT dev set is PubTables' VAL split, and the default kept
0 of its pages on both VMs (04-10). A run is one split: the flag takes one
value, and the manifest records it.

NAMED PAGES. `--pages-list FILE` fetches exactly the pages the file names --
one `document<TAB>page` line per page, zero-based, as in dev.json -- with no
per-document cap and no page count. The dev fetches pass it because the
first-3-pages-per-document cap kept "the first 3 table pages per document",
not the pages dev.json names, and 26 dev pages past 3 per document were
never asked for (attempt 9b, 04-10). It refuses together with `--pages`,
`--pages-per-document` and `--documents`: the list is the whole selection.
Every named page lands kept or refused with a named reason, in
`refused_pages.json` (`<document>-p<page:04d>` to reason), so kept plus
refused is the list itself: a page the tarball never held, a page with no
table above the floor, a cut that failed -- each is a page with a reason,
never a silent gap. The training draws pass no list and are unchanged.

Every kept page is one page -- the cut holds the article's page alone --
so the manifest says `"single_page": true` beside the article's
`page_no`: the evals read page 0 of the kept file, never the article's
page of it.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import shutil
import sys
import tarfile

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parents[1]))
sys.path.insert(0, str(HERE.parents[2] / "ci" / "loop"))
import pubtables_cells  # noqa: E402
import structured_bench as bench  # noqa: E402

STORE = pathlib.Path.home() / "Datasets" / "pdfluent-training" / "pubtables"
FLOOR_GB = 40.0


def free_gb(path) -> float:
    """The free gigabytes on the filesystem holding `path`: the fetch writes
    under --out, so that is the filesystem whose floor matters, on macOS and
    on Linux alike."""
    return shutil.disk_usage(path).free / 1e9


def wanted_documents(path: pathlib.Path) -> set[str]:
    """The documents a --documents file names: one PMC id per line, `#`
    comments and blank lines skipped."""
    wanted = set()
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        wanted.add(line)
    return wanted


def read_pages_list(path: pathlib.Path) -> dict[str, set[int]]:
    """The pages a --pages-list file names: document to zero-based pages.

    One `document<TAB>page` line per page, as in dev.json; `#` comments
    and blank lines skipped. Anything else refuses naming the line: a
    guessed page is a page the gate cannot account for.
    """
    wanted: dict[str, set[int]] = {}
    for number, raw in enumerate(
            path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if len(fields) != 2 or not fields[0].strip():
            raise ValueError(
                f"{path} line {number}: one document<TAB>page per "
                f"line, not {raw.strip()!r}")
        try:
            page = int(fields[1].strip())
        except ValueError:
            raise ValueError(
                f"{path} line {number}: the page is zero-based, not "
                f"{fields[1].strip()!r}")
        if page < 0:
            raise ValueError(
                f"{path} line {number}: the page is zero-based, not "
                f"{page}")
        wanted.setdefault(fields[0].strip(), set()).add(page)
    return wanted


def explicitly_passed(argv: list[str], flag: str) -> bool:
    """Whether `argv` names `flag`, as `--flag value` or `--flag=value`."""
    return any(token == flag or token.startswith(flag + "=")
               for token in argv)


def make_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pages", type=int, default=300)
    parser.add_argument("--out", default=str(STORE / "pdf"))
    parser.add_argument("--pages-list", default=None,
                        help="fetch exactly the pages this file names (one "
                             "document<TAB>page line per page, zero-based), "
                             "with no per-document cap and no page count; "
                             "refuses together with --pages, "
                             "--pages-per-document and --documents")
    parser.add_argument("--documents", default=None,
                        help="fetch only the documents this file names (one "
                             "PMC id per line, `#` comments), instead of the "
                             "first N in tarball order")
    parser.add_argument("--annotations", default=str(
        STORE / "PubTables-1M-PDF_Annotations.tar.gz"))
    parser.add_argument("--pages-per-document", type=int, default=3,
                        help="at most this many table pages per document")
    parser.add_argument("--split", choices=("train", "val"), default="train",
                        help="keep the annotations of this PubTables split "
                             "only: train for the training draws (the "
                             "default), val for the dev fetches, whose set "
                             "is the VAL split")
    return parser


def main(argv: list[str] | None = None) -> int:
    words = list(sys.argv[1:] if argv is None else argv)
    args = make_parser().parse_args(words)

    out = pathlib.Path(args.out)
    (out / "pages").mkdir(parents=True, exist_ok=True)
    articles = out / "articles"
    articles.mkdir(parents=True, exist_ok=True)

    pages_wanted = None
    if args.pages_list:
        for flag in ("--pages", "--pages-per-document", "--documents"):
            if explicitly_passed(words, flag):
                print(f"REFUSED: --pages-list fetches exactly the pages "
                      f"it names, with no page count and no per-document "
                      f"cap: it takes no {flag}", file=sys.stderr)
                return 2
        try:
            pages_wanted = read_pages_list(pathlib.Path(args.pages_list))
        except ValueError as exc:
            print(f"REFUSED: {exc}", file=sys.stderr)
            return 2
        except OSError as exc:
            print(f"REFUSED: --pages-list {args.pages_list} is not "
                  f"readable: {exc}", file=sys.stderr)
            return 2
        print(f"fetching {sum(map(len, pages_wanted.values()))} named "
              f"pages in {len(pages_wanted)} documents from "
              f"{args.pages_list}", flush=True)

    wanted = None
    if args.documents:
        wanted = wanted_documents(pathlib.Path(args.documents))
        print(f"fetching {len(wanted)} named documents from "
              f"{args.documents}", flush=True)
    if pages_wanted is not None:
        wanted = set(pages_wanted)
    seen_wanted = set()

    entry = bench.dataset_entry("pubtables-1m")
    bucket = entry["documents_bucket"].rstrip("/")
    allowed = set(entry["documents_licence_codes"])

    rig = json.loads((HERE.parents[3] / "corpus" / "structured"
                      / "PUBTABLES1M.json").read_text())
    refused_documents = {page["document"] for page in rig["pages"]}
    print(f"the rig holds {len(refused_documents)} documents; none of them is "
          f"fetched here", flush=True)

    kept, refused = [], {}
    refused_pages: dict[str, str] = {}

    def refuse_page(pmcid: str, index: int, reason: str) -> None:
        """One named page refused with its reason: the gate's accounting."""
        refused_pages[f"{pmcid}-p{index:04d}"] = reason
        refused[reason] = refused.get(reason, 0) + 1

    def refuse_hit(pmcid: str, reason: str) -> None:
        """One document's refusal: every named page of it under a pages
        list -- so kept plus refused stays the list itself -- one count
        on the draws' path, as today."""
        if pages_wanted is not None:
            for index in sorted(pages_wanted[pmcid]):
                refuse_page(pmcid, index, reason)
        else:
            refused[reason] = refused.get(reason, 0) + 1

    with tarfile.open(args.annotations, "r|gz") as tar:
        for member in tar:
            if pages_wanted is None and len(kept) >= args.pages:
                break
            if free_gb(out) < FLOOR_GB:
                print(f"STOPPING: {free_gb(out):.1f} GB free is under the "
                      f"{FLOOR_GB:.0f} GB floor", file=sys.stderr)
                break
            if not member.name.endswith("_tables.json"):
                continue
            pmcid = member.name.split("_")[0]
            if wanted is not None and pmcid not in wanted:
                if pages_wanted is None:
                    refused["not in the documents file"] = refused.get(
                        "not in the documents file", 0) + 1
                continue
            if wanted is not None:
                seen_wanted.add(pmcid)
            if pmcid in refused_documents:
                refuse_hit(pmcid, "a rig document")
                continue
            handle = tar.extractfile(member)
            if handle is None:
                if pages_wanted is not None:
                    refuse_hit(pmcid, "unreadable annotation")
                continue
            try:
                annotation = json.load(handle)
            except json.JSONDecodeError:
                refuse_hit(pmcid, "unreadable annotation")
                continue
            if not any(t.get("split") == args.split for t in annotation):
                reason = ("not the training split" if args.split == "train"
                          else "not the validation split")
                refuse_hit(pmcid, reason)
                continue
            by_page = pubtables_cells.tables_by_page(annotation)
            if not by_page:
                refuse_hit(pmcid, "no table above the floor")
                continue

            record = bench.article_record(bucket, pmcid)
            why_not = bench.may_fetch(record, allowed)
            if why_not:
                refuse_hit(pmcid, why_not)
                continue
            blob = bench._get(record["pdf"], timeout=180)
            if not blob:
                refuse_hit(pmcid, "the PDF did not answer")
                continue
            article = articles / f"{pmcid}.pdf"
            article.write_bytes(blob)

            kept_this_document = 0
            if pages_wanted is not None:
                shortlist = sorted(set(by_page) & pages_wanted[pmcid])
            else:
                shortlist = sorted(by_page)
            for index in shortlist:
                if pages_wanted is None and len(kept) >= args.pages:
                    break
                # The cap is the draws' rule: the pages list names its
                # own pages, and every one of them is fetched.
                if pages_wanted is None:
                    if kept_this_document >= args.pages_per_document:
                        break
                page = out / "pages" / f"{pmcid}-p{index:04d}.pdf"
                if not bench.merge_document([str(article), str(index + 1)], page):
                    if pages_wanted is not None:
                        refuse_page(pmcid, index,
                                    "the page could not be cut from its "
                                    "article")
                    continue
                if not bench._aligned(by_page[index], bench._page_text(page)):
                    page.unlink(missing_ok=True)
                    if pages_wanted is not None:
                        refuse_page(pmcid, index,
                                    "the page does not carry its table's "
                                    "text")
                    else:
                        refused["the page does not carry its table's text"] = (
                            refused.get("the page does not carry its table's "
                                        "text", 0) + 1)
                    continue
                kept.append({
                    "name": page.name, "document": pmcid, "page_no": index,
                    "single_page": True,
                    "licence": record["licence"],
                    "sha256": hashlib.sha256(page.read_bytes()).hexdigest(),
                })
                kept_this_document += 1
            if pages_wanted is not None:
                for index in sorted(pages_wanted[pmcid] - set(by_page)):
                    refuse_page(pmcid, index, "no table above the floor")
            article.unlink(missing_ok=True)
            if len(kept) % 50 == 0 and kept:
                print(f"  {len(kept)} pages, {free_gb(out):.1f} GB free",
                      flush=True)

    missing = sorted(wanted - seen_wanted) if wanted is not None else []
    if pages_wanted is not None:
        for document in missing:
            for index in sorted(pages_wanted[document]):
                refuse_page(document, index,
                            "not in the annotation tarball")
    if missing:
        print(f"{len(missing)} named document(s) are not in the annotation "
              f"tarball: {', '.join(missing[:8])}"
              + (" ..." if len(missing) > 8 else ""), file=sys.stderr)
    (out / "MANIFEST.json").write_text(json.dumps({
        "set": "pubtables-1m", "split": args.split,
        "revision": entry.get("revision"),
        "licence_codes": sorted(allowed),
        "why": ("the train/serve gap of #600: our word boxes against "
                "theirs, on documents the rig does not hold"
                if args.split == "train" else
                "the T5 dev baseline: the PT dev set's pages, which are "
                "the VAL split, on documents the rig does not hold"),
        "rig_documents_excluded": len(refused_documents),
        "documents_file": args.documents,
        "pages_list": args.pages_list,
        "missing_documents": missing,
        "pages": kept, "refused": refused,
    }, indent=2) + "\n")
    if pages_wanted is not None:
        (out / "refused_pages.json").write_text(json.dumps(
            refused_pages, indent=2, sort_keys=True) + "\n")
    print(f"kept {len(kept)} pages, {free_gb(out):.1f} GB free")
    print("refused:", refused)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
