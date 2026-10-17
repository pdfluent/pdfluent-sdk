#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The PubTables truth file the trainers read (--pubtables-truth) (#638).

For each kept page of a fetch manifest, the page's tables from the annotation
tarball: `source`, `name`, `document`, `page_no`, `path`, and `blocks` of
`{kind: "table", text, html, order, bbox}`, as `pubtables_cells.tables_by_page`
and `text_of` give them. The current truth file was made by a throwaway script
with exactly this output; this is the real one.

One streaming pass over the tarball: a wanted document's tables are written
out the moment its annotation is read, so memory never holds the tarball.
"""
from __future__ import annotations

import argparse
import collections
import json
import pathlib
import sys
import tarfile

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parents[1]))
import pubtables_cells  # noqa: E402

STORE = pathlib.Path.home() / "Datasets" / "pdfluent-training" / "pubtables"
MANIFEST = STORE / "pdf" / "MANIFEST.json"
TARBALL = STORE / "PubTables-1M-PDF_Annotations.tar.gz"
PAGES = STORE / "pdf" / "pages"
OUT = STORE / "pdf" / "TRUTH.json"


def make_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--manifest", default=str(MANIFEST))
    parser.add_argument("--annotations", default=str(TARBALL))
    parser.add_argument("--pages-dir", default=str(PAGES))
    parser.add_argument("--out", default=str(OUT))
    return parser


def truth_of(manifest_pages: list[dict], pages_dir: pathlib.Path,
             annotations: dict[str, list[dict]],
             unreadable: set[str] | None = None,
             ) -> tuple[list[dict], int]:
    """The truth entries for the manifest's pages, in manifest order.

    A page carries its own page's tables: `page_no` is the annotation's
    0-based `pdf_page_index`, as the fetch wrote it. A manifest document the
    tarball never held -- or held unreadably, which was already said when the
    tarball was read -- is skipped, and a page with no table above the floor
    is kept with no blocks: said out loud, never guessed at.
    """
    out, missing = [], 0
    for page in manifest_pages:
        annotation = annotations.get(page["document"])
        if annotation is None:
            if not unreadable or page["document"] not in unreadable:
                print(f"no annotation for {page['document']}: "
                      f"{page['name']} is not in the truth file",
                      file=sys.stderr)
            missing += 1
            continue
        by_page = pubtables_cells.tables_by_page(annotation)
        tables = by_page.get(page["page_no"], [])
        if not tables:
            print(f"no table above the floor on {page['name']}: kept with "
                  "no blocks", file=sys.stderr)
        out.append({
            "source": "pubtables-1m/train",
            "name": page["name"],
            "document": page["document"],
            "page_no": page["page_no"],
            "path": str(pages_dir / page["name"]),
            "blocks": [{
                "kind": "table",
                "text": pubtables_cells.text_of(found["html"]),
                "html": found["html"],
                "order": order,
                "bbox": found["bbox"],
            } for order, found in enumerate(tables)],
        })
    return out, missing


def main(argv: list[str] | None = None) -> int:
    args = make_parser().parse_args(argv)
    manifest = json.loads(pathlib.Path(args.manifest).read_text(
        encoding="utf-8"))
    manifest_pages = manifest["pages"]
    wanted = {page["document"] for page in manifest_pages}
    pages_dir = pathlib.Path(args.pages_dir)

    # The manifest's pages grouped by document, so each annotation is read
    # once and released: the tarball streams and never seeks back.
    by_document: dict[str, list[dict]] = collections.defaultdict(list)
    for page in manifest_pages:
        by_document[page["document"]].append(page)
    entries: dict[str, list[dict]] = {}
    unreadable: set[str] = set()
    with tarfile.open(args.annotations, "r|gz") as tar:
        for member in tar:
            if len(entries) + len(unreadable) >= len(wanted):
                break
            if not member.name.endswith("_tables.json"):
                continue
            pmcid = member.name.split("_")[0]
            if pmcid not in wanted or pmcid in entries or pmcid in unreadable:
                continue
            handle = tar.extractfile(member)
            if handle is None:
                continue
            try:
                annotation = json.load(handle)
            except json.JSONDecodeError:
                print(f"unreadable annotation for {pmcid}: its pages are "
                      "not in the truth file", file=sys.stderr)
                unreadable.add(pmcid)
                continue
            entries[pmcid] = annotation
    truth, missing = truth_of(manifest_pages, pages_dir, entries, unreadable)
    pathlib.Path(args.out).write_text(json.dumps(truth, indent=2) + "\n",
                                      encoding="utf-8")
    print(f"wrote {len(truth)} pages of {len(wanted)} documents: {args.out}",
          flush=True)
    if missing:
        print(f"{missing} page(s) have no annotation and are not in the "
              "truth file", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
