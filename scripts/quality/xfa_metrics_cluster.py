#!/usr/bin/env python3
"""xfa_metrics_cluster.py — cluster XFA Quality Metrics v2 records by signature.

Milestone: XFA_CLUSTER_ANALYSIS_PAGE_UNDERPRODUCTION_AND_LOW_RECALL

ANALYSIS ONLY. Reads a Metrics v2 JSONL (one record per doc x policy; see
metrics_v2_record.schema.json) and assigns each doc a cluster label from observable
signatures (pages vs oracle, recall/precision, admitted_nodes, input_class, validity).
Emits a sanitized summary (counts + capped 8-hex doc-id prefixes). Changes no behavior.

Cluster labels:
  UNDER_PAGINATED_LOW_RECALL, SAME_PAGE_LOW_RECALL, EXTRA_CONTENT_HIGH_PRECISION_DROP,
  OVER_PAGINATED, UNDER_PAGINATED_RECALL_OK, REPAIRABILITY_NOT_LAYOUT,
  OUTPUT_INVALID_VALID_INPUT, OK_OR_NEUTRAL

Usage:
  xfa_metrics_cluster.py --jsonl /path/outside/repo/golden.jsonl [--recall-low 0.85]
                         [--precision-low 0.85] [--cap 12]
"""

import argparse
import json
from collections import Counter, defaultdict


def pick_fm(policy_map):
    return policy_map.get("FreshMergeExperimental") or policy_map.get("SavedStateFaithful")


def label(r, rlow, plow):
    iv, ov = r.get("input_class"), r.get("output_valid")
    pcd, rec, prec = r.get("page_count_delta"), r.get("text_recall"), r.get("text_precision")
    if not ov and iv != "valid_pdf":
        return "REPAIRABILITY_NOT_LAYOUT"
    if not ov:
        return "OUTPUT_INVALID_VALID_INPUT"
    if pcd is not None and pcd < 0:
        return "UNDER_PAGINATED_LOW_RECALL" if (rec is not None and rec < rlow) else "UNDER_PAGINATED_RECALL_OK"
    if pcd is not None and pcd > 0:
        return "OVER_PAGINATED"
    if rec is not None and rec < rlow:
        return "SAME_PAGE_LOW_RECALL"
    if prec is not None and prec < plow:
        return "EXTRA_CONTENT_HIGH_PRECISION_DROP"
    return "OK_OR_NEUTRAL"


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--jsonl", required=True, help="Metrics v2 JSONL (outside the repo)")
    ap.add_argument("--recall-low", type=float, default=0.85)
    ap.add_argument("--precision-low", type=float, default=0.85)
    ap.add_argument("--cap", type=int, default=12, help="max prefixes listed per cluster")
    args = ap.parse_args()

    by = defaultdict(dict)
    with open(args.jsonl) as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            r = json.loads(line)
            by[r["doc_id_prefix"]][r.get("policy", "?")] = r
    docs = {p: pick_fm(m) for p, m in by.items()}

    counts = Counter()
    members = defaultdict(list)
    for p, r in docs.items():
        lab = label(r, args.recall_low, args.precision_low)
        counts[lab] += 1
        if len(members[lab]) < args.cap:
            members[lab].append(p)

    out = {
        "docs": len(docs),
        "thresholds": {"recall_low": args.recall_low, "precision_low": args.precision_low},
        "cluster_counts": dict(counts),
        "members_capped": {k: sorted(v) for k, v in members.items()},
    }
    print(json.dumps(out, indent=2))


if __name__ == "__main__":
    main()
