#!/usr/bin/env python3
"""Python binding overhead: open+page_count loop + malformed loop, per-iter ns percentiles.
Usage: python_perf.py <validPdfPath> <iters>  (pdfluent.open_pdf takes a PATH)."""
import sys, json, time, resource, tempfile, os
import pdfluent
valid = sys.argv[1]
iters = int(sys.argv[2]) if len(sys.argv) > 2 else 400
# malformed temp file (open_pdf is path-based)
mf = tempfile.NamedTemporaryFile(suffix=".pdf", delete=False)
mf.write(b"%PDF-1.7\nnot a real pdf \xde\xad\xbe\xef"); mf.close()
bad = mf.name

def pct(a, q): return a[min(len(a) - 1, int(q * (len(a) - 1)))]

def loop(path, n):
    t = []; oks = errs = 0; pages = -1
    for _ in range(n):
        a = time.perf_counter_ns()
        try:
            d = pdfluent.open_pdf(path)
            pages = d.page_count() if callable(getattr(type(d), "page_count", None)) else d.page_count
            oks += 1
        except Exception:
            errs += 1
        t.append(time.perf_counter_ns() - a)
    t.sort()
    return {"samples": len(t), "oks": oks, "errs": errs, "pages": pages,
            "min_ns": t[0], "p50_ns": pct(t, .5), "p95_ns": pct(t, .95), "p99_ns": pct(t, .99), "max_ns": t[-1]}

for _ in range(30):
    try: pdfluent.open_pdf(valid)
    except Exception: pass
v = loop(valid, iters); m = loop(bad, iters)
os.unlink(bad)
rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
print(json.dumps({"binding": "python", "identity": pdfluent.__file__, "valid": v, "malformed": m, "rss_kb": rss, "status": "green_measured"}))
