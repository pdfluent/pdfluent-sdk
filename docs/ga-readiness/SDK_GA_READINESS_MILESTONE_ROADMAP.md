# SDK GA Readiness — Milestone Roadmap (non-XFA)

Sequenced milestones to take the non-XFA SDK from "all gates green at
compile/parity level" to **enterprise-ready GA**. Each milestone is a
separate `/goal`-style execution run with its own verdict. XFA fidelity is
a parallel track; this roadmap does not depend on it (except the shared
package-naming/claims checks in DX-9/DX-2).

Ordering principle: **close adoption + claim prerequisites first (DX), then
the enterprise safety assurances (Quality), then the durability mechanism
(Performance budgets), then packaging dry-run on real artifacts, then the
GA claims review.** Quality has more release-blockers, but a chunk of them
are cheap extensions of green primitives and can run in parallel with DX.

---

## Milestone A — DX 100% closure

- **Goal:** Every supported language has an accurate, CI-verified path from
  install → first success → error recovery, with docs that cannot drift.
- **Why ordered here:** DX gaps are *prerequisites for any GA claim and for
  external adoption/eval*. They are also the cheapest high-leverage wins and
  unblock the packaging/claims milestones (D, E). Nothing in A depends on B/C.
- **Inputs:** [DEVELOPER_EXPERIENCE_GA_MODEL.md](DEVELOPER_EXPERIENCE_GA_MODEL.md); matrix rows DX-1..DX-12; existing golden-path + drift + parity artifacts.
- **Deliverables:** per-language quickstart pages; expanded docs drift checker (snippets+feature-matrix+install+error-catalogue); consumer-style first-run smoke per binding; public feature-matrix ↔ internal-capability-matrix consistency check; license UX docs; CHANGELOG + semver policy; troubleshooting guide.
- **Gates:** `check_examples_and_snippets.py` (expanded scope) green; per-binding consumer smoke green; feature-matrix consistency check green; binding/license parity still green; no private paths.
- **Stop conditions:** any quickstart that cannot reach output on a clean env; any docs claim without matrix backing; package-name inconsistency surfaced (hand to D, do not fake).
- **Parallelizable:** quickstarts, troubleshooting, changelog, reference-completeness can run in parallel; drift-checker expansion must land before the doc rows are certified.
- **Fixes allowed:** YES — docs/examples/checker code + tiny binding-smoke harness; no Core behavior changes.
- **Branch:** `quality/sdk-ga-dx-100`
- **Final verdict:** `SDK_GA_DX_100_PERCENT_GREEN` or `SDK_GA_DX_BLOCKED_<reason>`.

## Milestone B — Quality / Reliability 100% closure

- **Goal:** Prove the enterprise safety envelope: no-panic everywhere incl.
  FFI, deterministic typed errors mapped per binding, memory-sound FFI,
  concurrency model, no-network, external structural validation, and a
  CI-wired non-XFA regression corpus.
- **Why ordered here:** these are the assurances enterprises *buy*; they are
  release-blocking and the largest cluster (12 rows). Sequenced after A
  because A makes the product adoptable/claimable; B makes it trustworthy.
- **Inputs:** [QUALITY_RELIABILITY_GA_MODEL.md](QUALITY_RELIABILITY_GA_MODEL.md); matrix QR-1..QR-15; existing security/limits/error tests + fixture manifest.
- **Deliverables:** per-method hostile-input matrix; FFI panic→typed-error guards; labelled malformed + encryption + bomb corpora; Send/Sync assertions + concurrency stress; ASAN/LSAN/Miri FFI harness + ownership doc; per-binding exception-mapping tests; qpdf/veraPDF output-validation gate; zero-egress test; non-XFA regression-corpus CI gate; redaction/signature/active-content security proofs.
- **Gates:** all new test suites green; sanitizer harness clean; corpus gate green in CI; structural-validator gate green; existing gates still green.
- **Stop conditions:** any sanitizer-detected UB/leak; any binding swallowing errors; any network egress on a non-license path; corpus regression.
- **Parallelizable:** QR-1/2/3/6 (extensions of green primitives) can run parallel with QR-8/9/11 (new harnesses); QR-14 corpus underpins others so start early.
- **Fixes allowed:** YES — tests + harnesses + bounded safety fixes; larger behavior fixes get spun out as their own issues, not silently bundled.
- **Branch:** `quality/sdk-ga-quality-100`
- **Final verdict:** `SDK_GA_QUALITY_100_PERCENT_GREEN` or `SDK_GA_QUALITY_BLOCKED_<reason>`.

## Milestone C — Performance baseline + regression budgets

- **Goal:** Standardized perf report + per-op baselines + per-op budgets
  wired to a CI regression gate, across core and bindings; plus the no-leak
  assertion (shared with B).
- **Why ordered here:** perf has the deepest `missing_evidence` but only the
  no-leak row is GA-blocking; the rest is enterprise-ready depth. It is
  sequenced after B because B establishes the corpus/harness plumbing C
  reuses, and because optimizing before correctness is proven is premature.
- **Inputs:** [PERFORMANCE_GA_MODEL.md](PERFORMANCE_GA_MODEL.md); matrix PF-1..PF-15; `core_pdf_perf_gate.py`, `pdf-bench` crate, WASM perf rounds.
- **Deliverables:** `perf_report.schema.json`; per-op/bucket baselines (open/parse/save/extract/render/split/merge/rotate); memory/RSS + no-leak harness; WASM size + browser-init timing; per-binding overhead + cold-start; budget JSON + CI regression gate with tolerance.
- **Gates:** perf gate green vs committed baselines; no-leak assertion green; size/init within ceilings; report conforms to schema.
- **Stop conditions:** a measured regression beyond tolerance with no committed re-baseline rationale; a leak.
- **Parallelizable:** schema (PF-15) first, then all measurement issues parallel; budget gate (PF-14) last.
- **Fixes allowed:** harness/measurement only; performance *optimization* fixes are post-GA unless a budget is breached (then spun out).
- **Branch:** `quality/sdk-ga-performance-baseline`
- **Final verdict:** `SDK_GA_PERFORMANCE_BASELINE_GREEN` or `SDK_GA_PERFORMANCE_BLOCKED_<reason>`.

## Milestone D — Packaging / release-candidate dry-run

- **Goal:** Build real artifacts for all channels and re-run every
  `green_but_needs_release_recheck` row against them; resolve package-naming
  consistency.
- **Why ordered here:** requires A (docs/install accuracy) and B/C (the gates
  it re-runs on artifacts). It converts dev-commit greens into
  artifact-proven greens.
- **Inputs:** `audit-all-packages.sh`, `package_cabi.sh`, `transform-wasm-pkg.sh`, `release_train_guard.sh`; DX-2/DX-11; license recheck.
- **Deliverables:** built artifacts per channel (no publish); audit green on built artifacts; install-from-local-artifact smoke per binding; package-name + license-text consistency report.
- **Gates:** `audit-all-packages.sh` (no NO_ARTIFACT); per-channel install smoke; license/version policy intact.
- **Stop conditions:** any channel artifact fails audit; any name/license inconsistency; **no publish/deploy/version bump** (hard stop).
- **Parallelizable:** per-channel builds parallel; consolidation last.
- **Fixes allowed:** packaging metadata only (no behavior, no version bump unless operator-approved at the GA cut).
- **Branch:** `quality/sdk-ga-packaging-rc-dryrun`
- **Final verdict:** `SDK_GA_PACKAGING_RC_DRYRUN_GREEN` or `SDK_GA_PACKAGING_BLOCKED_<reason>`.

## Milestone E — GA claims / release-readiness review

- **Goal:** Sign off that every public claim is backed by a green matrix row;
  produce the GA go/no-go.
- **Why ordered last:** it is the synthesis of A–D; claims can only be
  certified once their evidence exists.
- **Inputs:** all prior reports + the scoring matrix re-scored.
- **Deliverables:** claims↔evidence ledger; final maturity label per surface (beta/RC/GA-ready/enterprise-ready); GA go/no-go recommendation; operator decision memo.
- **Gates:** every claim has a `green_proven` (or release-resolved `green_but_needs_release_recheck`) row; no open release_blocker; no claim-blocking gap on advertised features.
- **Stop conditions:** any advertised claim without backing → claim must be removed or downgraded, not faked.
- **Parallelizable:** no (synthesis).
- **Fixes allowed:** docs/claims wording only; no code.
- **Branch:** `quality/sdk-ga-claims-readiness-review`
- **Final verdict:** `SDK_GA_READY` / `SDK_ENTERPRISE_READY` / `SDK_GA_NOGO_<reason>`.

---

## Sequencing summary

```
A (DX) ──► B (Quality) ──► C (Perf budgets) ──► D (Packaging RC dry-run) ──► E (Claims/GA review)
   │            │                                     ▲
   └─ parallel-friendly within each milestone         └─ D re-runs A/B/C gates on real artifacts
```

- A and the cheap B rows (QR-1/2/3/6) may overlap if capacity allows.
- C depends on B's corpus/harness plumbing.
- D depends on A+B+C; E depends on all.
- XFA fidelity runs in parallel and only intersects at DX-2/DX-9 (naming/claims) and E (claims review).
