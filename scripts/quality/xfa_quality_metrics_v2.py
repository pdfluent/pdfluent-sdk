#!/usr/bin/env python3
"""xfa_quality_metrics_v2.py — XFA/PDF Quality Metrics v2 collector.

Milestone: XFA_QUALITY_METRICS_V2_GOLDEN_BASELINE

MEASUREMENT ONLY. Wraps the existing `pdfluent measure` binary (no rendering/parser/
FreshMerge behavior change) and compares PDFluent's flattened output against golden
oracle assets (oracle text + oracle flattened PDF page count). Emits one sanitized
record per (doc, policy) conforming to metrics_v2_record.schema.json.

Per (doc, policy):
  - measure -> output_valid, page_count, text_objs, admitted_nodes, runtime_errors
  - pdftotext(PDFluent output) -> normalized my text + per-page chars/words
  - oracle text (pdfrest/speedtest/generic *_text.txt, else pdftotext of a flat oracle)
    -> doc-level recall/precision
  - oracle flat PDF page count -> page_count_delta
  - input_class reused from the repairability baseline jsonl (no reparse)
  - classify per the Phase-1 taxonomy

SAFETY: output/temp dirs MUST be outside the repo (enforced, fail-closed). Records carry
only an 8-hex doc_id_prefix; no full sha/path/oracle-text content is emitted.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import unicodedata
from collections import Counter

SHA_DIR_RE = re.compile(r"^[0-9a-f]{16,}$")
_WORD = re.compile(r"\w+", re.UNICODE)

TEXT_PROVIDERS = [("pdfrest_text.txt", "pdfrest"), ("speedtest_text.txt", "speedtest"),
                  ("text.txt", "generic")]
FLAT_PROVIDERS = [("pdfrest_flat.pdf", "pdfrest"), ("itext_flat.pdf", "itext"),
                  ("speedtest_flat.pdf", "speedtest")]

RECALL_LOW = 0.85
PRECISION_LOW = 0.85
IMPROVED_EPS = 0.02
VISUAL_BAND = (0.85, 0.95)
THRESHOLDS = {"recall_low": RECALL_LOW, "precision_low": PRECISION_LOW,
              "improved_eps": IMPROVED_EPS, "visual_band": list(VISUAL_BAND)}

POLICY_TOKEN = {"SavedStateFaithful": "saved-state", "FreshMergeExperimental": "fresh-merge"}


def run(cmd, timeout, want_bytes=False):
    try:
        r = subprocess.run(cmd, capture_output=True, timeout=timeout)
        out = r.stdout if want_bytes else (r.stdout.decode("utf-8", "replace"))
        return r.returncode, out
    except FileNotFoundError:
        return None, b"" if want_bytes else ""
    except subprocess.TimeoutExpired:
        return 124, b"" if want_bytes else ""


def doc_id_prefix(path):
    parent = os.path.basename(os.path.dirname(path))
    if SHA_DIR_RE.match(parent):
        return parent[:8]
    return "00000000"


def normalize_text(s):
    s = unicodedata.normalize("NFKC", s)            # also folds ligatures (fi, fl, ...)
    s = s.replace(" ", " ")                    # NBSP -> space
    s = "".join(c if (c in "\n\t" or ord(c) >= 32) else " " for c in s)  # strip controls
    return s.lower()


def words(s):
    return _WORD.findall(normalize_text(s))


def recall_precision(my_words, oracle_words):
    if not oracle_words:
        return (None, None)
    cm, co = Counter(my_words), Counter(oracle_words)
    inter = sum((cm & co).values())
    recall = inter / sum(co.values())
    precision = inter / sum(cm.values()) if cm else 0.0
    return (round(recall, 4), round(precision, 4))


def pdftotext(pdf_path, timeout):
    rc, out = run(["pdftotext", "-enc", "UTF-8", pdf_path, "-"], timeout)
    return out if rc == 0 or out else None


def page_texts(raw):
    pages = raw.split("\f")
    if pages and pages[-1].strip() == "":
        pages = pages[:-1]
    return pages


def pdfinfo_pages(pdf_path, timeout):
    rc, out = run(["pdfinfo", pdf_path], timeout)
    if rc is None:
        return None
    m = re.search(r"^Pages:\s+(\d+)", out, re.MULTILINE)
    return int(m.group(1)) if m else None


def resolve_oracle(golden_dir, timeout):
    """Return (oracle_words, text_provider, flat_page_count, flat_provider, oracle_available)."""
    text_path = text_prov = None
    for fn, prov in TEXT_PROVIDERS:
        cand = os.path.join(golden_dir, fn)
        if os.path.isfile(cand):
            text_path, text_prov = cand, prov
            break
    flat_path = flat_prov = None
    for fn, prov in FLAT_PROVIDERS:
        cand = os.path.join(golden_dir, fn)
        if os.path.isfile(cand):
            flat_path, flat_prov = cand, prov
            break

    oracle_words = None
    if text_path:
        try:
            with open(text_path, encoding="utf-8", errors="replace") as fh:
                oracle_words = words(fh.read())
        except OSError:
            oracle_words = None
    elif flat_path:                       # fall back to extracting oracle text from flat
        raw = pdftotext(flat_path, timeout)
        if raw is not None:
            oracle_words = words(raw)
            text_prov = flat_prov

    flat_pages = pdfinfo_pages(flat_path, timeout) if flat_path else None
    oracle_available = bool(text_path or flat_path)
    return oracle_words, text_prov, flat_pages, flat_prov, oracle_available


def measure(binary, input_pdf, policy, out_dir, doc_id, idx, timeout):
    mj = os.path.join(out_dir, f"{doc_id}_{idx}_{policy}.json")
    op = os.path.join(out_dir, f"{doc_id}_{idx}_{policy}.pdf")
    rc, _ = run([binary, "measure", "--input", input_pdf, "--policy", POLICY_TOKEN[policy],
                 "--output-json", mj, "--output-pdf", op, "--doc-id", doc_id], timeout)
    metrics = {}
    if os.path.isfile(mj):
        try:
            metrics = json.load(open(mj))
        except (OSError, ValueError):
            metrics = {}
    return metrics, (op if os.path.isfile(op) else None)


def classify(rec, oracle_suspect):
    if not rec["output_valid"]:
        if rec["input_class"] in ("strict_parser_rejects_repairable",
                                  "damaged_but_repairable", "damaged_not_repairable"):
            return "broken_input_repair_unsupported"
        return "output_invalid"
    if oracle_suspect:
        return "oracle_quality_suspect"
    if rec["page_count_delta"] not in (None, 0):
        return "page_count_delta"
    if rec["text_recall"] is not None and rec["text_recall"] < RECALL_LOW:
        return "missing_content"
    if rec["text_precision"] is not None and rec["text_precision"] < PRECISION_LOW:
        return "extra_content"
    if (rec["policy"] == "FreshMergeExperimental"
            and rec["recall_delta_vs_ssf"] is not None
            and rec["recall_delta_vs_ssf"] > IMPROVED_EPS
            and rec["page_count_delta"] in (None, 0)):
        return "improved"
    if (rec["text_recall"] is not None and VISUAL_BAND[0] <= rec["text_recall"] < VISUAL_BAND[1]):
        return "visual_review_needed"
    return "neutral"


def assert_outside_repo(path):
    p = os.path.abspath(path)
    anc = p
    while not os.path.exists(anc) and anc not in ("/", ""):
        anc = os.path.dirname(anc)
    rc, out = run(["git", "-C", anc, "rev-parse", "--show-toplevel"], 10)
    if rc == 0 and out.strip():
        root = os.path.abspath(out.strip())
        if p == root or p.startswith(root + os.sep):
            sys.exit(f"SAFETY ABORT: '{os.path.basename(p)}' is inside the repo. "
                     "Choose an output/temp path outside the repo.")


def load_repairability(path):
    m = {}
    if path and os.path.isfile(path):
        for line in open(path):
            line = line.strip()
            if line:
                try:
                    r = json.loads(line)
                    m[r["doc_id_prefix"]] = r.get("input_class", "unknown")
                except ValueError:
                    pass
    return m


def load_suspects(path):
    s = set()
    if path and os.path.isfile(path):
        for line in open(path):
            line = line.strip()
            if re.fullmatch(r"[0-9a-f]{8,}", line):
                s.add(line[:8])
    return s


def collect_pdfs(args):
    pdfs = []
    if args.golden_dir:
        for entry in sorted(os.listdir(args.golden_dir)):
            cand = os.path.join(args.golden_dir, entry, "input.pdf")
            if os.path.isfile(cand):
                pdfs.append(cand)
    if args.input_list:
        for line in open(args.input_list):
            line = line.strip()
            if line and os.path.isfile(line):
                pdfs.append(line)
    if args.limit:
        pdfs = pdfs[: args.limit]
    return pdfs


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--golden-dir")
    ap.add_argument("--input-list")
    ap.add_argument("--binary", required=True)
    ap.add_argument("--output-jsonl", required=True)
    ap.add_argument("--temp-dir", required=True)
    ap.add_argument("--policies", default="SavedStateFaithful,FreshMergeExperimental")
    ap.add_argument("--repairability-jsonl")
    ap.add_argument("--oracle-suspect-list")
    ap.add_argument("--limit", type=int)
    ap.add_argument("--timeout", type=int, default=60)
    args = ap.parse_args()

    if not args.golden_dir and not args.input_list:
        ap.error("provide --golden-dir and/or --input-list")
    assert_outside_repo(args.output_jsonl)
    assert_outside_repo(args.temp_dir)
    # Fail-closed binary preflight: a missing/broken binary must abort, not silently
    # classify every doc as output_invalid.
    rc, _ = run([args.binary, "measure", "--help"], 30)
    if rc != 0:
        sys.exit(f"ERROR: binary '{args.binary}' missing or 'measure' unavailable "
                 f"(rc={rc}). Build it first: cargo build --release -p xfa-cli")
    os.makedirs(args.temp_dir, exist_ok=True)
    os.makedirs(os.path.dirname(os.path.abspath(args.output_jsonl)), exist_ok=True)

    policies = [p.strip() for p in args.policies.split(",") if p.strip()]
    repair = load_repairability(args.repairability_jsonl)
    suspects = load_suspects(args.oracle_suspect_list)
    pdfs = collect_pdfs(args)
    if not pdfs:
        sys.exit("No PDFs found.")

    cls_counts = Counter()
    pol_cls_counts = {p: Counter() for p in policies}
    recall_buckets = Counter()
    page_delta_counts = Counter()
    oracle_present = 0
    n_docs = 0
    measure_no_metrics = 0

    with open(args.output_jsonl, "w") as out:
        for idx, path in enumerate(pdfs):
            n_docs += 1
            prefix = doc_id_prefix(path)
            gdir = os.path.dirname(path)
            input_class = repair.get(prefix, "unknown")
            is_suspect = prefix in suspects
            ow, tprov, flat_pages, fprov, oracle_avail = resolve_oracle(gdir, args.timeout)
            if oracle_avail:
                oracle_present += 1

            per_policy = {}
            for pol in policies:
                metrics, op = measure(args.binary, path, pol, args.temp_dir, prefix, idx,
                                      args.timeout)
                if not metrics:
                    measure_no_metrics += 1
                output_valid = bool(metrics.get("output_valid", False))
                page_count = metrics.get("page_count")
                my_words = my_chars = None
                per_page = None
                recall = precision = None
                if op:
                    raw = pdftotext(op, args.timeout)
                    if raw is not None:
                        mw = words(raw)
                        my_words, my_chars = len(mw), len(normalize_text(raw).strip())
                        per_page = []
                        for i, pt in enumerate(page_texts(raw), 1):
                            per_page.append({"page": i, "chars": len(normalize_text(pt).strip()),
                                             "words": len(words(pt))})
                        recall, precision = recall_precision(mw, ow)
                pcd = (page_count - flat_pages) if (page_count is not None and flat_pages is not None) else None
                per_policy[pol] = {
                    "doc_id_prefix": prefix, "policy": pol, "input_class": input_class,
                    "output_valid": output_valid, "page_count": page_count,
                    "oracle_flat_page_count": flat_pages, "page_count_delta": pcd,
                    "text_objs": metrics.get("text_ops"),
                    "my_chars": my_chars, "my_words": my_words,
                    "oracle_words": (len(ow) if ow is not None else None),
                    "text_recall": recall, "text_precision": precision,
                    "per_page": per_page,
                    "admitted_nodes": metrics.get("fresh_merge_admitted_nodes", 0),
                    "runtime_errors": metrics.get("runtime_errors", 0),
                    "oracle_text_provider": tprov, "oracle_flat_provider": fprov,
                    "oracle_available": oracle_avail, "recall_delta_vs_ssf": None,
                    "visual": None, "oracle_quality_suspect": is_suspect,
                }

            ssf = per_policy.get("SavedStateFaithful")
            fm = per_policy.get("FreshMergeExperimental")
            if fm and ssf and fm["text_recall"] is not None and ssf["text_recall"] is not None:
                fm["recall_delta_vs_ssf"] = round(fm["text_recall"] - ssf["text_recall"], 4)

            for pol in policies:
                rec = per_policy[pol]
                rec["classification"] = classify(rec, is_suspect)
                out.write(json.dumps(rec) + "\n")
                cls_counts[rec["classification"]] += 1
                pol_cls_counts[pol][rec["classification"]] += 1
                if pol == "FreshMergeExperimental" or len(policies) == 1:
                    if rec["page_count_delta"] is not None:
                        page_delta_counts["delta" if rec["page_count_delta"] else "match"] += 1
                    if rec["text_recall"] is not None:
                        b = ("1.0" if rec["text_recall"] >= 0.999 else
                             "0.95-1.0" if rec["text_recall"] >= 0.95 else
                             "0.85-0.95" if rec["text_recall"] >= 0.85 else
                             "0.5-0.85" if rec["text_recall"] >= 0.5 else "<0.5")
                        recall_buckets[b] += 1

    summary = {
        "docs": n_docs, "policies": policies, "oracle_present_docs": oracle_present,
        "oracle_missing_docs": n_docs - oracle_present,
        "measure_no_metrics_records": measure_no_metrics,
        "classification_counts_all_records": dict(cls_counts),
        "classification_by_policy": {p: dict(c) for p, c in pol_cls_counts.items()},
        "page_count_vs_oracle": dict(page_delta_counts),
        "recall_buckets": dict(recall_buckets),
        "thresholds": THRESHOLDS,
    }
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
