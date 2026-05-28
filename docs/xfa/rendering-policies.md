# XFA Rendering Policies

PDFluent flattens an XFA form by resolving a conflict between two views of the
form's content:

1. the **embedded form DOM** — the instance set the document was last saved with
   (for example, Adobe Reader's saved runtime state), and
2. a **fresh `template + datasets` re-merge** — recomputing which subforms/fields
   the data implies, independent of the saved state.

The choice between these is the **rendering policy**, modelled by
`pdf_xfa::XfaRenderingPolicy`.

> PDFluent does **not** claim a single universal "Adobe parity" mode. XFA rendering
> is policy-dependent (saved-state vs fresh-merge), mirroring Adobe's own
> static-vs-dynamic rendering distinction. Do not describe any policy as
> "Adobe-identical" or "pixel-perfect Adobe".

---

## `SavedStateFaithful` (default, production)

Honours the embedded form DOM: render the instance set the document was last saved
with, and suppress template/data instances the form DOM did not enumerate.

- **Default.** Selected automatically when no policy is specified.
- **Production-supported.** This is the policy the `pdfluent`/`xfa-cli flatten`
  command applies. It is byte-stable across releases for a given input.
- **When to use:** always, unless you have a specific, validated reason to recover
  data the saved state omitted. Sensitive / regulated workflows (legal, government,
  financial archival) should use SavedStateFaithful.

## `FreshMergeExperimental` (experimental, opt-in)

Ignores the saved form DOM for dynamic sections and **admits data-bound subforms
the form DOM omitted** (non-hidden, non-zero-instance, data-bound, not
`bind match="none"`).

- **Experimental and opt-in.** Never the default. It must be selected explicitly
  (`fresh-merge` / `FreshMergeExperimental`).
- **Not produced by `flatten`.** The `flatten` command applies SavedStateFaithful
  only and rejects a `fresh-merge` request. Exercise FreshMerge via the policy-aware
  API (`flatten_xfa_to_pdf_with_policy`) or `pdfluent measure --policy fresh-merge`.
- **May diverge from the embedded Adobe Reader saved state.** Because it recomputes
  the instance set, output (page count, content) can differ from what a viewer shows
  for the saved document. On some documents it recovers genuinely missing data rows;
  on others it can add empty/duplicate content or change page counts.
- **When to use:** only for investigation/measurement, or for a specific document
  class you have validated, and never as a silent default for third-party documents.

---

## Why both policies exist

Neither view is universally "correct":

- A document saved by a viewer may legitimately have **fewer** instances than the
  data implies (the user removed rows) — SavedStateFaithful preserves that intent.
- A document may have been generated with data but **never opened/saved by a viewer**,
  so the form DOM is empty/stale and the saved state under-represents the data —
  FreshMerge can recover the intended content.

Picking one global default would be wrong for the other case, so PDFluent keeps the
conservative, intent-preserving `SavedStateFaithful` as the default and exposes
`FreshMergeExperimental` as an explicit, opt-in alternative.

---

## What D13 will measure

D13 is the corpus-scale validation of `FreshMergeExperimental`. Using the
`pdfluent measure` entry point and the runner
(`scripts/xfa_fidelity/run_fresh_merge_policy_comparison.py`), it will, per document
and per policy, record page count, text-operator count, file size, admitted-node
count, output validity, and a classification (improved / neutral / likely-overrender
/ likely-blank-rows / etc.), then compare SavedStateFaithful vs FreshMergeExperimental
across designed cohorts (neutral control, D12 regression set, protected targets,
high-signal sets). It will flag protected-target deviations as hard blockers.

Status so far:
- **D12** validated FreshMerge on a 9-document target set (GREEN): no page-count
  regressions on 8 protected targets; `01de9ce4` recovered ~+80% text content.
- **D13 corpus-scale measurement is pending** until storage hardening completes
  (the CI runner / corpus-archival storage work). Until then, treat FreshMerge
  results as informational, not a production signal.

---

## What not to claim publicly

- ❌ "FreshMerge is the default" — it is not; SavedStateFaithful is.
- ❌ "FreshMerge is production-ready / validated at scale" — not until D13 completes.
- ❌ "Adobe parity" / "Adobe-identical" / "pixel-perfect Adobe" for any policy.
- ❌ "FreshMerge is always better" — it can over-render or duplicate content.
- ✅ Accurate: "SavedStateFaithful is the default, production policy;
  FreshMergeExperimental is an experimental, opt-in policy pending corpus-scale
  (D13) validation, and may diverge from the embedded saved form state."

---

## See also

- `crates/pdf-xfa/src/flatten.rs` — `XfaRenderingPolicy`, `flatten_xfa_to_pdf_with_policy*`.
- `benchmarks/runs/xfa_enterprise_plan/d12_fresh_merge_experimental_validation/` — D12 evidence.
- `benchmarks/runs/xfa_enterprise_plan/d13_fresh_merge_corpus_measurement_prep/` — D13 cohorts, schema, runner.
