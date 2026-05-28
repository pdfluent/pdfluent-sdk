#!/usr/bin/env python3
"""XFA-B-002 — Compare PDFluent flatten output to oracle, multi-metric.

Reads the truth table + the B-001 run log (and optional trace files) and produces
schema-conformant `batch_comparison_results.json`. Never relies on a single
metric. Uses a STRUCTURAL parser (pikepdf) for XFA/AcroForm retention — never a
raw-byte scan. Honours first-N rules (XFA-B-003): a `first_n_pages` record never
asserts a full-document page-count verdict.

No engine changes; no private artifacts produced (reads existing files; emits a
sanitized JSON whose only paths are server `/opt/...` paths or hashes).

Usage:
  compare_pdfluent_to_oracle.py --truth-table tt.json --run-log run_log.jsonl \
      --out batch_comparison_results.json
"""
import argparse, json, os, re, subprocess

PROVIDER_TEXT = {"pdfrest": "pdfrest_text.txt", "speedtest": "speedtest_text.txt"}


def pdfinfo_pages(path):
    try:
        out = subprocess.run(["pdfinfo", path], capture_output=True, text=True, timeout=60).stdout
        m = re.search(r"^Pages:\s+(\d+)", out, re.M)
        return int(m.group(1)) if m else None
    except Exception:
        return None


def pdftotext_chars(path):
    try:
        out = subprocess.run(["pdftotext", path, "-"], capture_output=True, text=True, timeout=120).stdout
        return len(out)
    except Exception:
        return None


def structural_retention(path):
    """Return (xfa_retained, acroform_retained) via structural parse. None on error."""
    try:
        import pikepdf
        with pikepdf.open(path) as pdf:
            root = pdf.Root
            acro = "/AcroForm" in root
            xfa = False
            if "/XFA" in root:
                xfa = True
            if acro and "/XFA" in root.AcroForm:
                xfa = True
            if not xfa:
                for obj in pdf.objects:
                    try:
                        if hasattr(obj, "keys") and "/XFA" in obj.keys():
                            xfa = True
                            break
                    except Exception:
                        pass
            return bool(xfa), bool(acro)
    except ImportError:
        return None, None
    except Exception:
        return None, None


def trace_stage_hint(trace_path):
    if not trace_path or not os.path.isfile(trace_path):
        return None
    try:
        t = json.load(open(trace_path))
        return t.get("stage_first_divergence_hint")
    except Exception:
        return None


def oracle_text_chars(record):
    src_dir = os.path.dirname(record["source_pdf_path"])
    fname = PROVIDER_TEXT.get(record["oracle_source"])
    if fname:
        p = os.path.join(src_dir, fname)
        if os.path.isfile(p):
            try:
                return len(open(p, "r", errors="ignore").read())
            except Exception:
                return None
    # fall back to extracting from the oracle flat
    return pdftotext_chars(record["oracle_flat_pdf_path"])


def reliability_class(record, out_ok):
    if not out_ok:
        return "inconclusive"
    if record["sensitivity"] in ("operator-private", "customer-private"):
        return "private-artifact-limited"
    if record["coverage"] == "first_n_pages":
        return "first-N-only"
    if record["coverage"] == "full_document":
        return "reliable"
    return "inconclusive"


def preclassify(rec):
    """Lightweight, non-visual preclassification (NOT a final defectclass)."""
    out = []
    pc_rel = rec["page_count_reliability"] == "reliable"
    po, pp = rec["page_count_oracle"], rec["page_count_pdfluent"]
    if pc_rel and po and pp:
        # A 1-page oracle flat vs a multi-page PDFluent output is far more likely
        # a failed/placeholder oracle render than a PDFluent over-pagination
        # (empirically: most speedtest 1-page flats). Flag oracle quality instead
        # of fabricating an over-pagination defect.
        if po == 1 and pp >= 3:
            out.append("oracle_quality_suspect")
        elif pp < po:
            out.append("under_pagination_candidate")
            out.append("page_count_mismatch")
        elif pp > po:
            out.append("over_pagination_candidate")
            out.append("page_count_mismatch")
    tr = rec["text_ratio"]
    if tr is not None and rec["oracle_scope"] != "first_n_pages":
        if tr < 0.5:
            out.append("missing_text_candidate")
        elif tr < 0.85:
            out.append("sparse_output_candidate")
    if rec["structural_xfa_retained"] is True:
        out.append("structural_xfa_retention")
    if rec["file_size_ratio"] and rec["file_size_ratio"] > 3.0:
        out.append("file_size_bloat")
    if rec["oracle_scope"] == "first_n_pages":
        out.append("first_n_inconclusive")
    if rec["error"]:
        out.append("runtime_failure")
    if not out:
        out.append("ok_or_negligible")
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--truth-table", required=True)
    ap.add_argument("--run-log", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    tt = {r["doc_id"]: r for r in json.load(open(args.truth_table))["records"]}
    runs = {}
    for line in open(args.run_log):
        line = line.strip()
        if not line:
            continue
        r = json.loads(line)
        runs[r["doc_id"]] = r

    results = []
    for doc_id, rec0 in tt.items():
        run = runs.get(doc_id)
        if run is None:
            continue  # not in this batch
        out_pdf = run.get("output_path")
        out_ok = run.get("exit") == 0 and out_pdf and os.path.isfile(out_pdf)
        po = pdfinfo_pages(rec0["oracle_flat_pdf_path"])
        pp = pdfinfo_pages(out_pdf) if out_ok else None
        psrc = pdfinfo_pages(rec0["source_pdf_path"])
        # page-count reliability: only when oracle coverage is full_document
        pc_reliability = "reliable" if (rec0["coverage"] == "full_document" and out_ok) else (
            "first_n_only" if rec0["coverage"] == "first_n_pages" else "inconclusive")
        toc = oracle_text_chars(rec0)
        tpp = pdftotext_chars(out_pdf) if out_ok else None
        tr = round(tpp / toc, 4) if (toc and tpp is not None and toc > 0) else None
        xfa_ret, acro_ret = structural_retention(out_pdf) if out_ok else (None, None)
        osz = run.get("output_size")
        try:
            orsz = os.path.getsize(rec0["oracle_flat_pdf_path"])
        except Exception:
            orsz = None
        fsr = round(osz / orsz, 3) if (osz and orsz) else None
        rec = {
            "oracle_record_id": doc_id,
            "doc_id": doc_id.split("::")[0],
            "provider": rec0["oracle_source"],
            "oracle_scope": rec0["coverage"],
            "first_n_pages": rec0.get("coverage_n"),
            "source_sha256": rec0.get("source_sha256"),
            "oracle_sha256": rec0.get("oracle_sha256"),
            "pdfluent_output_sha256": run.get("output_sha256"),
            "page_count_source": psrc,
            "page_count_oracle": po,
            "page_count_pdfluent": pp,
            "page_count_reliability": pc_reliability,
            "text_chars_oracle": toc,
            "text_chars_pdfluent": tpp,
            "text_ratio": tr,
            "text_comparison_label": ("first_n_text_only" if rec0["coverage"] == "first_n_pages" else "full_text"),
            "structural_xfa_retained": xfa_ret,
            "acroform_retained": acro_ret,
            "file_size_ratio": fsr,
            "trace_stage_hint": trace_stage_hint(run.get("trace_path")),
            "reliability_class": reliability_class(rec0, out_ok),
            "eligible_for_milestone_c": bool(rec0["oracle_pngs_available"] and rec0["can_render_locally"] and out_ok),
            "private_artifact_policy": ("report_only_no_commit"
                                        if not rec0["can_commit_derived_artifacts"] else "commit_ok"),
            "error": run.get("error"),
            "notes": None,
        }
        rec["defect_preclassification"] = preclassify(rec)
        # first-N guard: never assert a full-doc page-count verdict
        if rec0["coverage"] == "first_n_pages":
            rec["page_count_verdict"] = "not_applicable_first_n"
        else:
            rec["page_count_verdict"] = (
                "match" if (pp is not None and po is not None and pp == po)
                else ("mismatch" if (pp is not None and po is not None) else "unknown"))
        results.append(rec)

    out = {"schema_version": "1.0", "generated_count": len(results), "results": results}
    json.dump(out, open(args.out, "w"), indent=1)
    # summary to stdout
    from collections import Counter
    rel = Counter(r["reliability_class"] for r in results)
    print(json.dumps({"records": len(results), "reliability": dict(rel)}))


if __name__ == "__main__":
    main()
