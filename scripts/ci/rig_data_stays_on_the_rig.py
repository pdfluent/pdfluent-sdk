#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The rig's pages stay on the round machine (#574, incident of 29-09-2026).

    python3 scripts/ci/rig_data_stays_on_the_rig.py            # scan this machine
    import rig_data_stays_on_the_rig as rig; rig.refuse_here("what is running")
                                              rig.refuse_split("doclaynet", "test")

WHY. On 29-09-2026 a lane was found developing on 200 rig pages and 3,961
DocLayNet test-split pages kept on the Mac (Hugging Face test shards). A number
measured on pages somebody has iterated against is a statement about memory,
and the rig is only worth its machine-hour while its pages are unseen. The
rule was a sentence; this makes it a refusal, the way `feature_dump` refuses
the local ParseBench tuning copy.

WHAT IS REFUSED, OFF THE ROUND MACHINE (anything that is not Linux -- the round
machine is a bought Linux instance, and every lane develops on the Mac):
  * asking for DocLayNet's `test` split, the PubTables rig set, or the
    FinTabNet rig set (`refuse_split`);
  * running at all while a rig page sits on this machine (`refuse_here`): a file
    named like a page or a document of `corpus/structured/DOCLAYNET_TEST.json`
    or `PUBTABLES1M.json`, a page of ParseBench's SEALED half (its tuning half
    may be here, owner decision 29-09-2026), or a DocLayNet `test-*.parquet`
    shard, in the Hugging Face cache, ~/Datasets, ~/.local/share/pdfluent or
    the temp directories.

Exit 0 when the machine is clean, 1 naming what it found.
"""
from __future__ import annotations

import json
import os
import pathlib
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
MANIFESTS = REPO / "corpus" / "structured"
DEPTH = 7


def on_the_round_machine() -> bool:
    # RIG_GUARD_AS_MAC lets the test judge a Linux CI runner as the Mac.
    if os.environ.get("RIG_GUARD_AS_MAC"):
        return False
    return sys.platform.startswith("linux") or bool(os.environ.get("PDFLUENT_ROUND_MACHINE"))


def rig_names(manifests: pathlib.Path = MANIFESTS) -> set[str]:
    """File names a rig page or document would be kept under."""
    names: set[str] = set()
    for path in sorted(manifests.glob("*.json")):
        data = json.loads(path.read_text(encoding="utf-8"))
        parsebench = path.name.upper().startswith("PARSEBENCH")
        for entry in data.get("pages") or []:
            if not isinstance(entry, dict):
                continue
            if parsebench and entry.get("split") == "tuning":
                continue
            name = str(entry.get("name") or "")
            base = name.rsplit("/", 1)[-1]
            if base:
                names.add(base if base.endswith(".pdf") else base + ".pdf")
            doc = str(entry.get("document") or "")
            if doc and not parsebench and path.name.upper().startswith("PUBTABLES"):
                names.add(doc if doc.endswith(".pdf") else doc + ".pdf")
    return names


def places() -> list[pathlib.Path]:
    home = pathlib.Path.home()
    roots = [home / ".cache" / "huggingface", home / "Datasets",
             home / ".local" / "share" / "pdfluent", pathlib.Path("/tmp"),
             pathlib.Path(tempfile.gettempdir())]
    extra = os.environ.get("RIG_GUARD_PLACES")
    if extra:
        roots = [pathlib.Path(p) for p in extra.split(os.pathsep) if p]
    seen, out = set(), []
    for r in roots:
        try:
            key = r.resolve()
        except OSError:
            continue
        if key not in seen and r.is_dir():
            seen.add(key)
            out.append(r)
    return out


def offenders(roots: list[pathlib.Path], names: set[str]) -> list[str]:
    found: list[str] = []
    for root in roots:
        base_depth = len(root.parts)
        for dirpath, dirnames, filenames in os.walk(root, onerror=lambda e: None):
            here = pathlib.Path(dirpath)
            if len(here.parts) - base_depth >= DEPTH:
                dirnames[:] = []
            lowered = dirpath.lower()
            for f in filenames:
                if f in names:
                    found.append(str(here / f))
                elif (f.startswith("test-") and f.endswith(".parquet")
                      and "doclaynet" in lowered):
                    found.append(str(here / f))
            if len(found) >= 20:
                return found
    return found


def refuse_here(what: str) -> None:
    """Stop `what` on a machine that holds rig pages."""
    if on_the_round_machine():
        return
    found = offenders(places(), rig_names())
    if found:
        raise SystemExit(
            f"[rig-data] {what}: refused, this machine holds rig pages, which never leave "
            f"the round machine (#574):\n  " + "\n  ".join(found[:10])
            + ("\n  ..." if len(found) > 10 else "")
            + "\nDelete them; develop on DocLayNet validation (scripts/extraction/"
              "rig_like_proxy.py) and measure on the rig.")


def refuse_split(dataset: str, split: str | None) -> None:
    """Stop a request for the rig's own split anywhere but the round machine."""
    if on_the_round_machine():
        return
    if (dataset == "doclaynet" and split == "test") or dataset in (
            "pubtables-1m", "pubtables", "fintabnet"):
        raise SystemExit(
            f"[rig-data] {dataset} {split or ''}: the rig's pages are fetched on the round "
            "machine only (#574). Off it, use the validation split.")


def main() -> int:
    if on_the_round_machine():
        print("[rig-data] on the round machine: nothing to refuse.")
        return 0
    found = offenders(places(), rig_names())
    if found:
        print("[rig-data] FAIL: rig pages on this machine:\n  " + "\n  ".join(found[:20]),
              file=sys.stderr)
        return 1
    print(f"[rig-data] OK: no rig page in {len(places())} place(s) on this machine.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
