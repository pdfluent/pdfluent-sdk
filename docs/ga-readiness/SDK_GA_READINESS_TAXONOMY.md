# PDFluent SDK — GA Readiness Taxonomy

Status vocabulary for the non-XFA SDK GA program. Every status is a
**claim about evidence**, not a feeling. "Mostly green" is not a status.
A status may only be assigned if its evidence requirement is met and the
evidence is referenceable (a committed report, a test target, a script
exit code, a gate run).

## 1. Maturity labels

| Label | Meaning | Evidence requirement |
| --- | --- | --- |
| **beta** | Surface works on the golden path but has unproven edges; API may still move. | At least one passing smoke per surface; known gaps explicitly listed. |
| **release_candidate (RC)** | Feature-complete for v1, API frozen, no known release-blockers, but not all release gates have been *re-run on the release artifact*. | RFC-frozen API + all functional gates green on a recent commit; pending only the release-artifact re-check. |
| **GA-ready** | Safe for general production use; all release-blocking gates green on the actual release artifact; documented and supported. | Every `release_blocker` category = `green_proven`; DX + Quality release-blocking rows green; packaging dry-run green on the built artifact. |
| **enterprise-ready** | GA-ready **plus** the assurances enterprises buy: deterministic typed errors, no-panic guarantees, resource limits, privacy/no-telemetry, security-sensitive workflow proofs, perf budgets with regression gates, support/runbook docs. | GA-ready + Quality categories 1–15 green + Performance regression budgets enforced in CI + troubleshooting/support docs published. |

## 2. Per-category disposition labels

| Label | Meaning | Evidence requirement |
| --- | --- | --- |
| **green_proven** | Verified now, with a referenceable artifact. | Named test/gate/report + exit code or pass line. |
| **green_but_needs_release_recheck** | Proven on a dev commit; must be re-run on the release artifact (built wheel/jar/wasm/tarball) before GA. | Dev-commit evidence + explicit "re-run on artifact" note. |
| **partial_evidence** | Some proof exists but does not cover the category's acceptance criteria. | What exists + precisely what is missing. |
| **missing_evidence** | Plausibly fine but unproven. | Statement of what would prove it. |
| **known_gap** | Confirmed shortfall vs. the acceptance criteria. | Description + impact. |
| **explicit_v1_limitation** | Deliberately out of v1 scope, documented with developer guidance. | Scope decision reference + expected developer behavior. |
| **release_blocker** | A `known_gap`/`missing_evidence` that must be closed before GA. | The gap + why it blocks the GA claim. |
| **out_of_scope_xfa** | Belongs to the separate XFA fidelity track. | — (excluded here except where it touches package naming/docs claims). |

### Disposition → maturity coupling

- A surface is **GA-ready** only if it has **zero** open `release_blocker` rows and all release-blocking rows are `green_proven` or `green_but_needs_release_recheck` (the latter resolved at release).
- `partial_evidence` / `missing_evidence` on a release-blocking category ⇒ the surface is at most **RC**.
- `known_gap` on a release-blocking category ⇒ the surface is at most **beta** for that axis until closed.

## 3. Release-blocking vs. not

- **release_blocker**: ships-incorrectly or unsafe-without-it. Examples: a public entrypoint panics on hostile input; a binding leaks memory at the FFI boundary; an install command in the docs does not work; a typed error code is unstable across a release.
- **claim_blocking_gap**: not unsafe, but makes a *public marketing/docs claim* false until fixed (e.g. "works in the browser in <2s" with no measured WASM init time; "7 language bindings" where one has no published package). Claim-blocking gaps block the **claim**, not necessarily the **binary**.
- **post_GA_improvement**: desirable, non-blocking; scheduled after GA (e.g. additional perf optimization beyond budget, extra examples, nice-to-have ergonomics).

## 4. How labels apply per surface

The same vocabulary applies independently to each surface; a surface's
maturity is the **minimum** across its release-blocking categories.

| Surface | What "GA-ready" specifically requires |
| --- | --- |
| **Core SDK (Rust)** | Capability matrix `green_proven`; no-panic + typed errors + resource limits proven; perf baseline with budgets; API frozen (RFC 0001). |
| **XFA** | *out_of_scope_xfa here* — tracked separately; only its effect on package naming and docs claims is in scope. |
| **Editor** | Reported green by its own suite; in scope here only for binding/docs/claim consistency. |
| **bindings (C ABI, WASM, Node, Python, .NET, Java)** | API parity proven; first-run smoke per binding; exception/error mapping proven; published-package install proven (or labelled `green_but_needs_release_recheck`); binding overhead measured. |
| **packaging/release** | `audit-all-packages.sh` green on built artifacts; license text + package names consistent; SHA/version policy intact. |
| **docs/examples** | Quickstart per language; copy-paste examples compiled/run in CI; feature-matrix accuracy; drift checker green; claims backed by evidence. |

## 5. Evidence reference conventions

Every status row in the matrix must cite at least one of:
- a **test target** (`crate::module` or `tests/<file>.rs::<test>`),
- a **gate/script** + observed exit code or verdict line,
- a **committed report** path under `benchmarks/runs/...`,
- a **CI job** name.

A row with no citation defaults to `missing_evidence`, never `green_*`.
