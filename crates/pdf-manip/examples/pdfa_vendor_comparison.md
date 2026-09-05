# Commercial comparison protocol — round 2

Status: **not measured**. This environment has no configured Apryse SDK/license
or Nutrient Document Engine license/endpoint. Docker is installed but its daemon
is unavailable. No licenses were purchased, evaluation account created, or
corpus documents sent to a hosted service. Internal measurements do not establish
parity with either vendor.

The local adapters implement the vendors' documented conversion routes:
[Apryse Python PDF/A conversion](https://docs.apryse.com/core/guides/features/pdfa/convert/python)
and [Nutrient PDF/A conversion](https://www.nutrient.io/sdk/pdf-a-conversion/).
They are syntax-checked and exercise missing-access failures locally; live SDK
behavior remains unverified until access is supplied.

Apryse requires its `PDFNetPython3` module and `APRYSE_LICENSE_KEY` in the
process environment. Nutrient requires a licensed local Document Engine,
`NUTRIENT_API_TOKEN`, and the deployed image/version in `NUTRIENT_ENGINE_VERSION`;
`NUTRIENT_LOCAL_URL` defaults to `http://127.0.0.1:5000`. Only loopback endpoints
are accepted. Describe license type and any trial watermark with
`PDFA_VENDOR_LICENSE_DESCRIPTION`; never put license keys in reports or command
arguments. A full Document Engine deployment also needs its documented database
and storage configuration.

Both adapters request PDF/A-2b. Apryse uses `PDFACompliance` conversion and
nonlinearized `SaveAs`, without a separate optimizer. Nutrient uses `/api/build`,
vectorization enabled and rasterization fallback disabled to keep failed
conversions visible rather than replacing text with a page image. No additional
image compression, OCR, font replacement, or external repair is requested by the
adapter. SDK internal conversion behavior and default font subsetting must be
recorded for the measured version. These settings describe standard conversion;
they do not pretend to be a separately tuned vendor optimizer comparison.

Run each SDK in a fresh output directory, on the identical 300-file list and the
hash-pinned development and confirmation sets in `pdfa_round2_holdout.json` and
`pdfa_round2_confirmatory.json`. For example, from the repository root:

```sh
node benchmarks/pdfa/reproduce/compare.mjs \
  --corpus-dir target/pdfa-size/govdocs \
  --list benchmarks/pdfa/govdocs_sample_300.txt \
  --converter 'python3 crates/pdf-manip/examples/pdfa_vendor_adapter.py apryse {in} {out}' \
  --name apryse-2b --workdir target/pdfa-size/vendor-apryse
```

Repeat with `nutrient` and a separate directory; repeat both on the frozen
manifest inputs. The 93-file set has been used for repairs and the 21-file set
re-evaluated after general fixes; neither is an untouched final blind holdout. Use the exact same veraPDF, MuPDF, fonts, DPI, timeout, and metric
versions as the internal run. Feed saved outputs into `pdfa_round2_eval.py` for
all-page and ordered-text measurements, with round-2 outputs as `--before`.
Every requested input remains in the denominator, including adapter failures,
timeouts, encrypted files, missing outputs and missing validator verdicts.

Each output has a `.measurement.json` sidecar with hashes, explicit settings,
SDK version, wall time, peak RSS and status. Apryse RSS includes the in-process
SDK. Nutrient RSS is **client only**; collect server/container peak memory
separately and do not compare it with in-process SDK RSS. Neither sidecar status
nor an SDK's own success result replaces external veraPDF validation.

Publish per-document deltas on conformance, text retention/order, rendering and
bytes, plus runtimes and memory with their scope. Any separately optimized run
must have its own settings/version record and repeat every quality axis. Report
trial watermarks and any rasterization separately. Until these runs exist, the
commercial comparison remains incomplete.
