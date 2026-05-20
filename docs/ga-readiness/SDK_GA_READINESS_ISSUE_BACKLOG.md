# SDK GA Readiness — Issue Backlog (Milestones A/B/C)

- **Date:** 2026-05-20 · Machine-readable: [`sdk_ga_readiness_issue_backlog.json`](sdk_ga_readiness_issue_backlog.json)
- Concrete issues for the first three milestones. **Planning artifact only — no issue is executed in this milestone.**
- Counts: DX 12 · Quality 15 · Performance 15 · total 42


## Milestone A — DX 100%

### GA-DX-001 · Per-language 5-minute quickstart pages
- **axis/type:** DX / docs · **parallel:** yes · **merge allowed:** yes
- **scope in:** one quickstart page per language: rust,c,wasm,node,python,dotnet,java; each ends in a verified printed output
- **scope out:** deep tutorials; XFA examples
- **inputs:** pdfluent-examples/*; GOLDEN_PATH_REVALIDATION_REPORT.md
- **deliverables:** 7 quickstart pages; each snippet registered with the docs example checker
- **acceptance:** a cold dev reaches output in <=5 steps; every snippet compiled/run in CI
- **gates:** check_examples_and_snippets.py (expanded)
- **stop:** a quickstart cannot reach output on a clean env
- **dependencies:** GA-DX-012
- **done:** all 7 pages exist, snippets CI-verified, checker green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_dx_100/GA-DX-001_quickstarts.md`

### GA-DX-002 · Install instructions per package/channel + name consistency
- **axis/type:** DX / docs · **parallel:** yes · **merge allowed:** yes
- **scope in:** install command per channel (crates.io,npm,PyPI,NuGet,Maven,C-ABI tarball); package-name consistency vs GitLab migration
- **scope out:** actual publishing (Milestone D)
- **inputs:** audit-all-packages.sh; release scripts
- **deliverables:** install doc per channel; package-name consistency report
- **acceptance:** each install line resolves the audited package name; no stale npm/crate names
- **gates:** audit-all-packages.sh --dry-run; name-consistency check
- **stop:** a package name mismatch is found (record, hand to D, do not fake)
- **dependencies:** none
- **done:** install docs match audited names; consistency report green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_dx_100/GA-DX-002_install.md`

### GA-DX-003 · All docs snippets compiled/run in CI
- **axis/type:** DX / test · **parallel:** no · **merge allowed:** yes
- **scope in:** every fenced code block in quickstart+cookbook compiled or marked no-run+reason
- **scope out:** XFA snippets
- **inputs:** docs/cookbook; COOKBOOK_EXAMPLES_DRIFT_MATRIX.json
- **deliverables:** snippet extraction+compile harness; no-run annotations where justified
- **acceptance:** 100% of snippets either compiled/run or explicitly no-run
- **gates:** check_examples_and_snippets.py (full scope)
- **stop:** a snippet neither compiles nor is justified no-run
- **dependencies:** GA-DX-012
- **done:** checker covers all snippets and is green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_dx_100/GA-DX-003_snippets.md`

### GA-DX-004 · Binding API naming/ergonomic consistency doc
- **axis/type:** DX / docs · **parallel:** yes · **merge allowed:** yes
- **scope in:** document naming convention per language; document the 19 intentionally_unsupported per binding
- **scope out:** adding new APIs
- **inputs:** check_binding_api_parity.py
- **deliverables:** convention doc; per-binding unsupported-list doc
- **acceptance:** every binding follows its documented convention; all 19 unsupported documented
- **gates:** check_binding_api_parity.py
- **stop:** an undocumented divergence is found
- **dependencies:** none
- **done:** convention + unsupported docs published; parity still green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_dx_100/GA-DX-004_consistency.md`

### GA-DX-005 · Error-message actionability + cross-binding code audit
- **axis/type:** DX / report · **parallel:** yes · **merge allowed:** yes
- **scope in:** audit each Error variant message for actionability; confirm catalogue completeness
- **scope out:** per-binding mapping tests (QR-11)
- **inputs:** error_codes_stable.rs; docs/error_catalogue.md
- **deliverables:** message-quality report; catalogue completeness confirmation
- **acceptance:** every variant has an actionable message + catalogue entry
- **gates:** error_codes_stable.rs; error_catalogue_sync.sh
- **stop:** a variant lacks a catalogue entry or actionable message
- **dependencies:** GA-QR-011
- **done:** catalogue complete; messages reviewed; sync gate green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_dx_100/GA-DX-005_error_dx.md`

### GA-DX-006 · License activate/deactivate/status UX docs + example
- **axis/type:** DX / docs · **parallel:** yes · **merge allowed:** yes
- **scope in:** documented flow per binding; runnable example; offline/expired/invalid messaging
- **scope out:** license server internals
- **inputs:** check_license_e2e_parity.py; docs/licensing.md
- **deliverables:** license UX doc per binding; runnable activation example
- **acceptance:** a dev can activate/deactivate/check status from docs alone
- **gates:** check_license_e2e_parity.py; example checker
- **stop:** a documented flow does not run
- **dependencies:** none
- **done:** UX docs + example run; parity green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_dx_100/GA-DX-006_license_ux.md`

### GA-DX-007 · Consolidated troubleshooting guide
- **axis/type:** DX / docs · **parallel:** yes · **merge allowed:** yes
- **scope in:** symptom->cause->fix for common failures (native lib, features, password, WASM init, license)
- **scope out:** exhaustive error reference (catalogue covers that)
- **inputs:** docs/error_catalogue.md
- **deliverables:** one troubleshooting page cross-linked to error codes
- **acceptance:** covers the top failure classes; links resolve
- **gates:** link-check
- **stop:** 
- **dependencies:** GA-DX-005
- **done:** troubleshooting page published, links valid
- **report:** `benchmarks/runs/ga_readiness_3d/ga_dx_100/GA-DX-007_troubleshooting.md`

### GA-DX-008 · API reference completeness + missing_docs lint
- **axis/type:** DX / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** enable missing_docs (or equivalent) on public crates; per-binding reference presence
- **scope out:** private items
- **inputs:** docs/en/api-reference.md; rustdoc
- **deliverables:** missing_docs lint enabled; generated reference per binding
- **acceptance:** cargo doc warns->errors on missing public docs; reference builds
- **gates:** cargo doc (deny missing_docs)
- **stop:** a public item is undocumented and cannot be quickly documented
- **dependencies:** none
- **done:** missing_docs enforced; references generated
- **report:** `benchmarks/runs/ga_readiness_3d/ga_dx_100/GA-DX-008_api_ref.md`

### GA-DX-009 · Public feature matrix <-> internal capability matrix consistency
- **axis/type:** DX / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** cross-check public/website feature matrix vs CORE_PDF_SDK_CAPABILITY_MATRIX; XFA naming/claims overlap
- **scope out:** changing capabilities
- **inputs:** CORE_PDF_SDK_CAPABILITY_MATRIX.json; public docs/site matrix
- **deliverables:** consistency checker; corrected public matrix
- **acceptance:** no public feature claim without a matrix-backed row
- **gates:** check_core_pdf_sdk_quality.py; new consistency check
- **stop:** a public claim has no matrix backing (downgrade claim, do not fake)
- **dependencies:** none
- **done:** public matrix matches internal; checker green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_dx_100/GA-DX-009_feature_matrix.md`

### GA-DX-010 · CHANGELOG + semver/breaking-change policy + migration notes
- **axis/type:** DX / docs · **parallel:** yes · **merge allowed:** yes
- **scope in:** CHANGELOG for the v1 line; semver policy; migration notes for pre-GA API moves
- **scope out:** future roadmap
- **inputs:** fase-*-decisions; closure reports
- **deliverables:** CHANGELOG.md; versioning policy doc
- **acceptance:** every pre-GA API move has a migration note; policy documented
- **gates:** docs presence
- **stop:** 
- **dependencies:** none
- **done:** changelog + policy + migration notes published
- **report:** `benchmarks/runs/ga_readiness_3d/ga_dx_100/GA-DX-010_changelog.md`

### GA-DX-011 · Consumer-style first-run smoke per binding
- **axis/type:** DX / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** per binding: a project that depends on the (local) artifact and runs one op
- **scope out:** publishing (D)
- **inputs:** capability matrix bindings_non_xfa_smoke; WASM Node runtime proof
- **deliverables:** 7 consumer smokes (rust,c,wasm,node,python,dotnet,java)
- **acceptance:** each consumes the artifact and asserts correct output
- **gates:** per-binding consumer smoke in CI
- **stop:** a binding cannot be consumed as a dependency
- **dependencies:** GA-DX-002
- **done:** all consumer smokes green in CI
- **report:** `benchmarks/runs/ga_readiness_3d/ga_dx_100/GA-DX-011_first_run.md`

### GA-DX-012 · Expand docs drift checker scope + CI-enforce
- **axis/type:** DX / harness · **parallel:** no · **merge allowed:** yes
- **scope in:** expand checker to snippets+feature-matrix+install+error-catalogue; wire into CI
- **scope out:** XFA docs
- **inputs:** scripts/docs/check_examples_and_snippets.py
- **deliverables:** expanded checker; CI job
- **acceptance:** checker fails on any drift in the four scopes
- **gates:** check_examples_and_snippets.py (expanded)
- **stop:** a drift class cannot be mechanically checked (document why)
- **dependencies:** none
- **done:** expanded checker green and CI-wired
- **report:** `benchmarks/runs/ga_readiness_3d/ga_dx_100/GA-DX-012_drift_checker.md`


## Milestone B — Quality/Reliability 100%

### GA-QR-001 · No-panic across all public entrypoints + FFI guard
- **axis/type:** Quality / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** hostile-input matrix over every public facade method; FFI panic->typed-error guard per binding
- **scope out:** XFA paths
- **inputs:** core_pdf_quality.rs; processing_limits.rs
- **deliverables:** per-method no-panic test; FFI catch_unwind boundary per binding
- **acceptance:** no public entrypoint panics on corpus/fuzz input; FFI converts panic->error
- **gates:** new no-panic suite; binding boundary tests
- **stop:** a panic crosses an FFI boundary
- **dependencies:** GA-QR-014
- **done:** all public methods + FFI proven panic-safe
- **report:** `benchmarks/runs/ga_readiness_3d/ga_quality_100/GA-QR-001.md`

### GA-QR-002 · Labelled malformed/corrupt corpus runner
- **axis/type:** Quality / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** malformed corpus with expected typed outcome per fixture; recoverable vs fatal classification
- **scope out:** valid-PDF perf
- **inputs:** CORE_PDF_FIXTURE_MANIFEST; core_pdf_quality.rs
- **deliverables:** malformed corpus + runner asserting Error variant
- **acceptance:** every malformed class -> documented typed error, bounded time, no panic
- **gates:** malformed corpus gate
- **stop:** a class hangs or panics
- **dependencies:** GA-QR-014
- **done:** corpus runner green in CI
- **report:** `benchmarks/runs/ga_readiness_3d/ga_quality_100/GA-QR-002.md`

### GA-QR-003 · Encryption handler matrix
- **axis/type:** Quality / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** RC4/AESV2/AESV3 x owner/user x permissions; per-binding decrypt smoke
- **scope out:** DRM
- **inputs:** security.rs; facade encrypt/decrypt
- **deliverables:** encryption fixture matrix; per-binding decrypt smoke
- **acceptance:** all standard handlers open/refuse correctly w/ typed errors; permissions honored
- **gates:** encryption matrix test
- **stop:** a handler mis-decrypts or leaks
- **dependencies:** none
- **done:** encryption matrix green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_quality_100/GA-QR-003.md`

### GA-QR-004 · Hybrid + corrupt xref/objstm breadth
- **axis/type:** Quality / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** hybrid-reference + corrupt-xref-stream recovery fixtures
- **scope out:** valid-only perf
- **inputs:** core_pdf_quality.rs objstm/xref; pdf-syntax/xref.rs
- **deliverables:** fixtures + expected outcomes
- **acceptance:** objstm/xref roundtrip preserved; corrupt -> typed error
- **gates:** xref/objstm breadth test
- **stop:** a corrupt variant panics
- **dependencies:** GA-QR-002
- **done:** breadth fixtures green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_quality_100/GA-QR-004.md`

### GA-QR-005 · Resource-limit exposure per binding + heavy paths
- **axis/type:** Quality / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** per-binding limit-config; apply limits on render/extract paths; default-limit docs
- **scope out:** new limit types
- **inputs:** processing_limits.rs
- **deliverables:** per-binding limit smoke; render/extract limit tests; limits doc
- **acceptance:** documented defaults; configurable per surface; enforced on heavy paths
- **gates:** processing_limits.rs + binding tests
- **stop:** a heavy path ignores limits
- **dependencies:** none
- **done:** limits exposed+enforced+documented
- **report:** `benchmarks/runs/ga_readiness_3d/ga_quality_100/GA-QR-005.md`

### GA-QR-006 · Decompression/recursion bomb guards
- **axis/type:** Quality / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** zip-bomb (Flate ratio), nested-filter, recursive-reference bombs
- **scope out:** valid perf
- **inputs:** processing_limits.rs; CORE_PDF_SECURITY_HARDENING_REPORT.md
- **deliverables:** bomb fixtures + bounded mem/time assertions
- **acceptance:** all bomb classes bounded in mem+time -> typed limit error
- **gates:** bomb test suite
- **stop:** a bomb exhausts memory or time
- **dependencies:** GA-QR-005
- **done:** bomb suite green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_quality_100/GA-QR-006.md`

### GA-QR-007 · Error determinism (repeat + cross-platform)
- **axis/type:** Quality / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** same input -> same code across repeats and platforms
- **scope out:** message wording
- **inputs:** error_codes_stable.rs
- **deliverables:** determinism test; cross-platform CI check
- **acceptance:** codes stable across N runs and platforms; catalogue in sync
- **gates:** error_codes_stable.rs; error_catalogue_sync.sh
- **stop:** a code is nondeterministic
- **dependencies:** none
- **done:** determinism proven
- **report:** `benchmarks/runs/ga_readiness_3d/ga_quality_100/GA-QR-007.md`

### GA-QR-008 · Concurrency model + Send/Sync + stress
- **axis/type:** Quality / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** document threading model; Send/Sync static assertions; multi-thread stress
- **scope out:** async runtime
- **inputs:** facade public types
- **deliverables:** threading-model doc; static assertions; stress test
- **acceptance:** documented model; no data races; assertions compile
- **gates:** concurrency stress + static asserts
- **stop:** a data race or unsound shared use is found
- **dependencies:** none
- **done:** concurrency proven+documented
- **report:** `benchmarks/runs/ga_readiness_3d/ga_quality_100/GA-QR-008.md`

### GA-QR-009 · FFI memory-safety under sanitizers
- **axis/type:** Quality / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** ASAN/LSAN over C-ABI/Node/Python harness; Miri over pdf-capi unsafe; ownership contract doc
- **scope out:** pure-Rust safe code
- **inputs:** pdf-capi; binding smokes
- **deliverables:** sanitizer harness; ownership/lifetime doc
- **acceptance:** zero leaks/UB under sanitizers on FFI surface; ownership documented
- **gates:** sanitizer CI job
- **stop:** a leak/UAF/double-free is detected
- **dependencies:** none
- **done:** sanitizers clean on FFI
- **report:** `benchmarks/runs/ga_readiness_3d/ga_quality_100/GA-QR-009.md`

### GA-QR-010 · WASM runtime safety (trap/OOM/panic mapping)
- **axis/type:** Quality / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** hostile-input harness Node+headless browser; panic->JS exception; bounded memory
- **scope out:** editor UI
- **inputs:** EDITOR_HANDOFF_SDK_RUNTIME_PROOF.md
- **deliverables:** WASM hostile-input harness
- **acceptance:** no uncatchable traps; panics surface as JS exceptions; bounded memory
- **gates:** WASM safety harness
- **stop:** an uncatchable trap on hostile input
- **dependencies:** GA-QR-001
- **done:** WASM safety harness green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_quality_100/GA-QR-010.md`

### GA-QR-011 · Per-binding exception mapping
- **axis/type:** Quality / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** trigger each Error class per binding; assert mapped exception type + same stable code
- **scope out:** new errors
- **inputs:** binding parity; error_codes_stable.rs
- **deliverables:** per-binding error-mapping test; mapping doc
- **acceptance:** 1:1 documented mapping; codes stable; no swallowed errors
- **gates:** error-mapping tests per binding
- **stop:** a binding swallows or remaps a code
- **dependencies:** GA-DX-005
- **done:** mapping tests green all bindings
- **report:** `benchmarks/runs/ga_readiness_3d/ga_quality_100/GA-QR-011.md`

### GA-QR-012 · External structural validation of outputs
- **axis/type:** Quality / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** run qpdf --check / veraPDF on save/merge/split/flatten outputs
- **scope out:** XFA output fidelity
- **inputs:** merge.rs; validate_pdfa
- **deliverables:** output-validation gate over representative outputs
- **acceptance:** outputs pass an independent structural validator
- **gates:** qpdf/veraPDF gate
- **stop:** an output fails structural validation
- **dependencies:** none
- **done:** validator gate green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_quality_100/GA-QR-012.md`

### GA-QR-013 · No-network/no-telemetry proof
- **axis/type:** Quality / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** zero-egress test on open/parse/save/extract/render across bindings; dep phone-home scan; license path documented offline
- **scope out:** license server
- **inputs:** project policy
- **deliverables:** no-network test harness; dependency egress scan
- **acceptance:** provably zero egress on non-license paths; license path offline-capable+documented
- **gates:** no-network harness
- **stop:** any egress on a non-license path
- **dependencies:** none
- **done:** no-egress proven
- **report:** `benchmarks/runs/ga_readiness_3d/ga_quality_100/GA-QR-013.md`

### GA-QR-014 · Non-XFA regression corpus CI gate
- **axis/type:** Quality / harness · **parallel:** no · **merge allowed:** yes
- **scope in:** curated non-XFA corpus subset; golden-outcome runner (open/extract/render/save); CI gate
- **scope out:** XFA corpus
- **inputs:** CORE_PDF_FIXTURE_MANIFEST; VPS corpus; corpus-mini
- **deliverables:** corpus subset; runner; CI gate
- **acceptance:** corpus gate runs in CI; regressions fail; corpus documented; no private paths
- **gates:** corpus regression gate
- **stop:** a corpus path leaks private data
- **dependencies:** none
- **done:** corpus gate green in CI
- **report:** `benchmarks/runs/ga_readiness_3d/ga_quality_100/GA-QR-014.md`

### GA-QR-015 · Security-workflow proofs (redaction/sign/active-content)
- **axis/type:** Quality / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** redaction content-removal proof (extract-after-redact); signature verify matrix; active-content sanitization
- **scope out:** new security features
- **inputs:** pdf-redact; CORE_PDF_SECURITY_HARDENING_REPORT.md
- **deliverables:** redaction removal test; signature verify matrix; sanitization test
- **acceptance:** redaction removes underlying content; signatures verify correctly; dangerous actions stripped
- **gates:** security-workflow suite
- **stop:** redacted content is still extractable
- **dependencies:** none
- **done:** security workflows proven
- **report:** `benchmarks/runs/ga_readiness_3d/ga_quality_100/GA-QR-015.md`


## Milestone C — Performance baseline + budgets

### GA-PF-015 · Standardized perf report schema
- **axis/type:** Performance / harness · **parallel:** yes · **merge allowed:** yes
- **scope in:** define perf_report.schema.json (p50/p95,RSS,sizes per op/bucket/binding)
- **scope out:** optimization (post-GA unless budget breached)
- **inputs:** pdf-bench crate; core_pdf_perf_gate.py; CORE_PDF_PERFORMANCE_BASELINE.json
- **deliverables:** perf_report.schema.json; all benches emit to it
- **acceptance:** all benches conform to schema
- **gates:** perf gate vs committed baseline
- **stop:** a regression beyond tolerance without re-baseline rationale
- **dependencies:** none
- **done:** baseline+budget committed; gate green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_perf/GA-PF-015.md`

### GA-PF-001 · Open/parse latency baseline
- **axis/type:** Performance / benchmark · **parallel:** yes · **merge allowed:** yes
- **scope in:** isolated open/parse bench over buckets 1p/50p/500p/large; p50/p95
- **scope out:** optimization (post-GA unless budget breached)
- **inputs:** pdf-bench crate; core_pdf_perf_gate.py; CORE_PDF_PERFORMANCE_BASELINE.json
- **deliverables:** open/parse bench + baseline
- **acceptance:** p95 per bucket recorded + budgeted
- **gates:** perf gate vs committed baseline
- **stop:** a regression beyond tolerance without re-baseline rationale
- **dependencies:** GA-PF-015
- **done:** baseline+budget committed; gate green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_perf/GA-PF-001.md`

### GA-PF-002 · Save/write latency baseline
- **axis/type:** Performance / benchmark · **parallel:** yes · **merge allowed:** yes
- **scope in:** save-only timing incl objstm + compression variants
- **scope out:** optimization (post-GA unless budget breached)
- **inputs:** pdf-bench crate; core_pdf_perf_gate.py; CORE_PDF_PERFORMANCE_BASELINE.json
- **deliverables:** save bench + baseline
- **acceptance:** p95 per bucket+variant recorded+budgeted
- **gates:** perf gate vs committed baseline
- **stop:** a regression beyond tolerance without re-baseline rationale
- **dependencies:** GA-PF-015
- **done:** baseline+budget committed; gate green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_perf/GA-PF-002.md`

### GA-PF-003 · Text extraction throughput baseline
- **axis/type:** Performance / benchmark · **parallel:** yes · **merge allowed:** yes
- **scope in:** full-doc chars/sec; layout vs raw
- **scope out:** optimization (post-GA unless budget breached)
- **inputs:** pdf-bench crate; core_pdf_perf_gate.py; CORE_PDF_PERFORMANCE_BASELINE.json
- **deliverables:** extract bench + baseline
- **acceptance:** chars/sec floor per bucket
- **gates:** perf gate vs committed baseline
- **stop:** a regression beyond tolerance without re-baseline rationale
- **dependencies:** GA-PF-015
- **done:** baseline+budget committed; gate green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_perf/GA-PF-003.md`

### GA-PF-004 · Render/thumbnail latency baseline
- **axis/type:** Performance / benchmark · **parallel:** yes · **merge allowed:** yes
- **scope in:** ms/page at 72/150 DPI; thumbnail latency
- **scope out:** optimization (post-GA unless budget breached)
- **inputs:** pdf-bench crate; core_pdf_perf_gate.py; CORE_PDF_PERFORMANCE_BASELINE.json
- **deliverables:** render bench + baseline
- **acceptance:** ms/page p95 per DPI budgeted
- **gates:** perf gate vs committed baseline
- **stop:** a regression beyond tolerance without re-baseline rationale
- **dependencies:** GA-PF-015
- **done:** baseline+budget committed; gate green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_perf/GA-PF-004.md`

### GA-PF-005 · Split/merge/rotate throughput baseline
- **axis/type:** Performance / benchmark · **parallel:** yes · **merge allowed:** yes
- **scope in:** pages/sec over buckets; large-merge memory
- **scope out:** optimization (post-GA unless budget breached)
- **inputs:** pdf-bench crate; core_pdf_perf_gate.py; CORE_PDF_PERFORMANCE_BASELINE.json
- **deliverables:** split/merge/rotate bench + baseline
- **acceptance:** pages/sec floor budgeted
- **gates:** perf gate vs committed baseline
- **stop:** a regression beyond tolerance without re-baseline rationale
- **dependencies:** GA-PF-015
- **done:** baseline+budget committed; gate green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_perf/GA-PF-005.md`

### GA-PF-006 · Memory peak/RSS + no-leak harness
- **axis/type:** Performance / test · **parallel:** yes · **merge allowed:** yes
- **scope in:** peak RSS per op/bucket; flat RSS over N iterations (no leak)
- **scope out:** optimization (post-GA unless budget breached)
- **inputs:** pdf-bench crate; core_pdf_perf_gate.py; CORE_PDF_PERFORMANCE_BASELINE.json
- **deliverables:** RSS sampler harness + no-leak assertion
- **acceptance:** peak-RSS ceiling per bucket; no leak over iterations
- **gates:** perf gate vs committed baseline
- **stop:** a regression beyond tolerance without re-baseline rationale
- **dependencies:** GA-PF-015, GA-QR-009
- **done:** baseline+budget committed; gate green ; no-leak proven
- **report:** `benchmarks/runs/ga_readiness_3d/ga_perf/GA-PF-006.md`

### GA-PF-007 · WASM binary size budget
- **axis/type:** Performance / benchmark · **parallel:** yes · **merge allowed:** yes
- **scope in:** track .wasm + gz/br size with ceiling
- **scope out:** optimization (post-GA unless budget breached)
- **inputs:** pdf-bench crate; core_pdf_perf_gate.py; CORE_PDF_PERFORMANCE_BASELINE.json
- **deliverables:** size record + ceiling in transform-wasm-pkg.sh
- **acceptance:** size within ceiling; fail on >X% growth
- **gates:** perf gate vs committed baseline
- **stop:** a regression beyond tolerance without re-baseline rationale
- **dependencies:** GA-PF-015
- **done:** baseline+budget committed; gate green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_perf/GA-PF-007.md`

### GA-PF-008 · WASM browser load/init timing
- **axis/type:** Performance / benchmark · **parallel:** yes · **merge allowed:** yes
- **scope in:** headless-browser fetch+instantiate+first-op timing
- **scope out:** optimization (post-GA unless budget breached)
- **inputs:** pdf-bench crate; core_pdf_perf_gate.py; CORE_PDF_PERFORMANCE_BASELINE.json
- **deliverables:** Playwright init-time bench + baseline
- **acceptance:** init-time p95 ceiling
- **gates:** perf gate vs committed baseline
- **stop:** a regression beyond tolerance without re-baseline rationale
- **dependencies:** GA-PF-015
- **done:** baseline+budget committed; gate green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_perf/GA-PF-008.md`

### GA-PF-009 · Binding overhead (Node/Python/.NET/Java)
- **axis/type:** Performance / benchmark · **parallel:** yes · **merge allowed:** yes
- **scope in:** per-binding overhead % vs native Rust for one op
- **scope out:** optimization (post-GA unless budget breached)
- **inputs:** pdf-bench crate; core_pdf_perf_gate.py; CORE_PDF_PERFORMANCE_BASELINE.json
- **deliverables:** per-binding overhead bench
- **acceptance:** overhead ceiling per binding
- **gates:** perf gate vs committed baseline
- **stop:** a regression beyond tolerance without re-baseline rationale
- **dependencies:** GA-PF-015
- **done:** baseline+budget committed; gate green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_perf/GA-PF-009.md`

### GA-PF-010 · C ABI overhead
- **axis/type:** Performance / benchmark · **parallel:** yes · **merge allowed:** yes
- **scope in:** C call overhead vs direct Rust
- **scope out:** optimization (post-GA unless budget breached)
- **inputs:** pdf-bench crate; core_pdf_perf_gate.py; CORE_PDF_PERFORMANCE_BASELINE.json
- **deliverables:** C micro-bench
- **acceptance:** overhead ceiling
- **gates:** perf gate vs committed baseline
- **stop:** a regression beyond tolerance without re-baseline rationale
- **dependencies:** GA-PF-015
- **done:** baseline+budget committed; gate green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_perf/GA-PF-010.md`

### GA-PF-011 · Large PDF performance baseline
- **axis/type:** Performance / benchmark · **parallel:** yes · **merge allowed:** yes
- **scope in:** 100MB+/1000p+ timing + RSS (curated big fixtures, no private paths)
- **scope out:** optimization (post-GA unless budget breached)
- **inputs:** pdf-bench crate; core_pdf_perf_gate.py; CORE_PDF_PERFORMANCE_BASELINE.json
- **deliverables:** large-file bench + baseline
- **acceptance:** time+RSS ceilings for large bucket
- **gates:** perf gate vs committed baseline
- **stop:** a regression beyond tolerance without re-baseline rationale
- **dependencies:** GA-PF-015, GA-QR-014
- **done:** baseline+budget committed; gate green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_perf/GA-PF-011.md`

### GA-PF-012 · Objstm/xref perf-impact baseline
- **axis/type:** Performance / benchmark · **parallel:** yes · **merge allowed:** yes
- **scope in:** parse-time delta classic vs xref-stream vs objstm of one doc
- **scope out:** optimization (post-GA unless budget breached)
- **inputs:** pdf-bench crate; core_pdf_perf_gate.py; CORE_PDF_PERFORMANCE_BASELINE.json
- **deliverables:** comparative bench (informational)
- **acceptance:** baseline recorded (no hard budget)
- **gates:** perf gate vs committed baseline
- **stop:** a regression beyond tolerance without re-baseline rationale
- **dependencies:** GA-PF-015
- **done:** baseline+budget committed; gate green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_perf/GA-PF-012.md`

### GA-PF-013 · Cold-start per binding
- **axis/type:** Performance / benchmark · **parallel:** yes · **merge allowed:** yes
- **scope in:** Python import, JVM/.NET startup, WASM instantiate + first op
- **scope out:** optimization (post-GA unless budget breached)
- **inputs:** pdf-bench crate; core_pdf_perf_gate.py; CORE_PDF_PERFORMANCE_BASELINE.json
- **deliverables:** cold-start bench per binding
- **acceptance:** cold-start ceiling per binding
- **gates:** perf gate vs committed baseline
- **stop:** a regression beyond tolerance without re-baseline rationale
- **dependencies:** GA-PF-015
- **done:** baseline+budget committed; gate green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_perf/GA-PF-013.md`

### GA-PF-014 · Per-op budgets + CI regression gate
- **axis/type:** Performance / blocker · **parallel:** yes · **merge allowed:** yes
- **scope in:** budget JSON per op/bucket; gate compares measured vs budget w/ tolerance; commit baseline
- **scope out:** optimization (post-GA unless budget breached)
- **inputs:** pdf-bench crate; core_pdf_perf_gate.py; CORE_PDF_PERFORMANCE_BASELINE.json
- **deliverables:** budget gate wired to CI
- **acceptance:** gate fails on regression beyond tolerance; baselines committed
- **gates:** perf gate vs committed baseline
- **stop:** a regression beyond tolerance without re-baseline rationale
- **dependencies:** GA-PF-001, GA-PF-002, GA-PF-003, GA-PF-004, GA-PF-005, GA-PF-006, GA-PF-007, GA-PF-008, GA-PF-009, GA-PF-010, GA-PF-013, GA-PF-015
- **done:** baseline+budget committed; gate green
- **report:** `benchmarks/runs/ga_readiness_3d/ga_perf/GA-PF-014.md`
