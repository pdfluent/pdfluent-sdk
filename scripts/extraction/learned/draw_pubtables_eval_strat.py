#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The PubTables eval strat-300 measure set, drawn once (#638).

300 PAGES from the existing eval pages -- the eval list's documents whose
pages are in the fetch manifest -- stratified over the PMC id in 250,000-id
buckets, at most one page per document. Drawn once, `--seed 638`, and
committed as `pubtables_eval_strat300.txt`.

FOR MEASURING ONLY: never read by a trainer. The trainers refuse it by name,
as they refuse the eval list. The EVAL list stays frozen, and its first 300
pages (PubTables-300) stay the continuity set; this set measures what that
one cannot, an era at a time.
"""
from __future__ import annotations

import argparse
import json
import os
import pathlib
import random
import sys

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
import pubtables_splits as splits  # noqa: E402

STORE = pathlib.Path.home() / "Datasets" / "pdfluent-training" / "pubtables"
EVAL_LIST = HERE.parent / "pubtables_eval_documents.txt"
MANIFEST = STORE / "pdf" / "MANIFEST.json"
OUT = HERE.parent / "pubtables_eval_strat300.txt"


def make_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--eval-list", default=str(EVAL_LIST))
    parser.add_argument("--manifest", default=str(MANIFEST))
    parser.add_argument("--count", type=int, default=300)
    parser.add_argument("--seed", type=int, default=638)
    parser.add_argument("--out", default=str(OUT))
    return parser


def one_page_per_document(grouped: dict[str, list[str]], seed: int,
                           ) -> dict[str, str]:
    """One page per document: the seeded pick of the document's sorted page
    names, so the draw is deterministic and no document speaks twice."""
    rng = random.Random(seed)
    return {document: rng.choice(sorted(names))
            for document, names in sorted(grouped.items())}


def header(count: int, seed: int, eval_list: str, manifest: str,
           report: dict[int, tuple[int, int]]) -> str:
    """The draw, named: the seed, the inputs, the buckets and their counts."""
    lines = [
        f"# PubTables eval strat-300: {count} page names, one per line, in "
        "id order.",
        f"# Drawn once, --seed {seed}: one page per document from the eval "
        "list's documents whose pages are in the fetch manifest, stratified "
        f"over the PMC id in {splits.BUCKET:,}-id buckets.",
        f"# Eval list: {eval_list}; manifest: {manifest}.",
        "# FOR MEASURING ONLY: never read by a trainer. The trainers refuse "
        "this set by name, as they refuse the eval list (#638).",
        "# Buckets (PMC range: drawn/candidates):",
    ]
    for bucket in sorted(report):
        lo, hi = bucket * splits.BUCKET, (bucket + 1) * splits.BUCKET - 1
        drawn, cands = report[bucket]
        lines.append(f"#   PMC {lo}-{hi}: {drawn}/{cands}")
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    args = make_parser().parse_args(argv)
    eval_list = os.path.expanduser(args.eval_list)
    manifest_path = os.path.expanduser(args.manifest)
    eval_docs = splits.read_id_set(pathlib.Path(eval_list))
    splits.require_file(pathlib.Path(manifest_path), "the fetch manifest")
    manifest = json.loads(pathlib.Path(manifest_path).read_text(
        encoding="utf-8"))
    grouped: dict[str, list[str]] = {}
    skipped = 0
    for page in manifest["pages"]:
        if page["document"] not in eval_docs:
            skipped += 1
            continue
        grouped.setdefault(page["document"], []).append(page["name"])
    missing = sorted(eval_docs - set(grouped))
    print(f"{len(grouped)} eval documents with pages in the manifest, "
          f"{skipped} non-eval pages ignored", flush=True)
    if missing:
        print(f"{len(missing)} eval document(s) have no page in the "
              f"manifest: {', '.join(missing[:8])}"
              + (" ..." if len(missing) > 8 else ""), file=sys.stderr)
    chosen = one_page_per_document(grouped, args.seed)
    drawn_docs, report = splits.stratified_draw(sorted(chosen), args.count,
                                                args.seed)
    if len(drawn_docs) < args.count:
        print(f"only {len(drawn_docs)} eval documents for --count "
              f"{args.count}: the list holds what there is", file=sys.stderr)
    names = sorted((chosen[doc] for doc in drawn_docs),
                   key=lambda n: (splits.pmc_number(splits.document_of(n))
                                  or 0, n))
    out = pathlib.Path(os.path.expanduser(args.out))
    out.write_text(header(len(names), args.seed, args.eval_list,
                          args.manifest, report)
                   + "".join(n + "\n" for n in names), encoding="utf-8")
    print(f"drew {len(names)} pages over {len(report)} buckets: {out}",
          flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
