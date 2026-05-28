# PDFluent CLI — Status & Roadmap Audit

- Date: 2026-05-22
- Branch: `quality/pdfluent-cli-status-and-roadmap-audit`
- Base: `origin/enterprise/ga-hardening` @ `1cc3d3d05`
- Scope: audit/report only (+ one isolated docs-only correction to the M1/M2 runbook; no CLI code, no publish, no binary rename).

## VERDICT: `PDFLUENT_CLI_CORE_PRESENT_BUT_NOT_PUBLIC_BETA_READY`

The public `pdfluent-cli` crate exists, builds, runs, has real SDK-backed core commands and
22 passing tests, with correct experimental/XFA caveats and `publish = false`. It is **not
yet public-beta-ready**: no packaging/distribution, `xfa flatten` is an explicit stub, and
the `pdfluent` binary-name takeover is deferred. No conflicts with the internal `xfa-cli`.

## 1. CLI crates
| Crate | Role | publish | Binaries |
|---|---|---|---|
| `crates/pdfluent-cli` | **public** CLI scaffold | `false` | `pdfluent-cli` |
| `crates/xfa-cli` | internal tooling (XFA runners, D13/QM) | (workspace) | **`pdfluent`** + `xfa-collector`, `edge-case-report`, `accuracy-report`, `edge-case-analyzer`, `xfa-license-tool`, `corpus-render`, `font-cache-builder` |

**No binary-name conflict today:** public CLI = `pdfluent-cli`; internal = `pdfluent`. The
`pdfluent` takeover is explicitly future work (and would require repointing XFA runners /
D13/QM tooling that depend on `target/release/pdfluent`).

## 2. Commands: real vs stub, SDK-backed?
| Command | Status | Calls real SDK? |
|---|---|---|
| `info` | implemented | yes — `PdfDocument::open`, `page_count`, `version`, file size |
| `inspect` | implemented (= `info --json`) | yes |
| `extract-text` | implemented (`--out`, `--json`) | yes — `doc.extract_text()` |
| `validate` | implemented | yes — parse + `page_count`; **explicitly NOT** PDF/A/veraPDF (honest caveat in help + JSON `compliance: null`) |
| `doctor` | implemented | no SDK call — build/env info only (offline, no telemetry) |
| `completions` | implemented | n/a — `clap_complete` |
| `xfa flatten` | **STUB** | no — returns `NOT_IMPLEMENTED`; `--policy fresh-merge` returns `EXPERIMENTAL_OPT_IN_REQUIRED` without `--experimental`; SSF default; correct "no Adobe parity" caveat |

## 3. Tests (`crates/pdfluent-cli/tests/cli.rs`, 22 passing)
Covers: `--help` lists commands + mentions product; `--version` == `1.0.0-beta.8`; `doctor`
exits 0 + experimental note; **internal commands not exposed**; xfa help has experimental
caveat; unknown command fails (non-zero); **fresh-merge requires `--experimental`**
(`EXPERIMENTAL_OPT_IN_REQUIRED`); `info` on a valid fixture. Good behavioral coverage of the
public contract + caveats. Gaps: no `extract-text --out` round-trip assertion; no
`completions` output assertion; no malformed-PDF error-envelope test for `extract-text`/`validate`.

## 4. Validation run (safe; no corpus, no publish)
- `cargo metadata` OK · `cargo build -p pdfluent-cli` OK · `cargo test -p pdfluent-cli` → **22 passed**.
- `--help` and per-command `--help` for all 7 commands render correctly.
- Functional smoke: `info fixtures/sample.pdf` → Pages 1 / PDF 1.7 / 578 bytes; `doctor` OK;
  `xfa flatten` → `NOT_IMPLEMENTED` (stub, as expected).

## 5. What's missing before **public beta-ready**
1. **Packaging/distribution** (the `publish = false` follow-up): crates.io or binary release
   plan, install docs, license/NOTICE in the package.
2. **Error-envelope + round-trip tests** for `extract-text --out` and malformed inputs.
3. Decide `xfa flatten` story for beta: keep as documented stub (current) vs hide behind a
   feature flag — current stub is honest and acceptable for beta.
4. `completions` output smoke test.

## 6. What's missing before **`pdfluent` binary takeover**
- Repoint every internal consumer of `target/release/pdfluent` (XFA runners, D13/QM measure
  tooling, CI scripts) to the renamed internal binary, OR keep `xfa-cli`'s `pdfluent` and
  ship the public CLI under a distinct distribution name. Inventory + migration is its own milestone.
- No takeover should happen until the public CLI is beta-ready and packaged.

## 7. Conflicts with XFA runners / release tooling
- **None today** (distinct binary names). The only collision risk is the *future* `pdfluent`
  takeover — flagged above as a separate, gated milestone.

## 8. M1/M2 runbook accuracy (Phase-4 of this audit)
**Yes — the M1/M2 runbook is now partially incorrect.** It describes
**M1 = `XFA_LAYOUT_INSTANCE_EXPANSION_PARITY`** ("fix Sub-B layout instance under-build").
XFA2's executed M1 report (`benchmarks/runs/.../xfa/layout-instance-expansion-parity`, merged
at `1cc3d3d05`) returns verdict **`XFA_LAYOUT_INSTANCE_EXPANSION_PARITY_NO_SAFE_FIX_THIS_ROUND`**:
a trace-driven investigation **disproved the premise** — instance expansion is **not** the
cause of Sub-B under-production; the real root cause is **layout overflow/pagination**
(over-full content not broken onto new pages; `content_area_with_overflow` /
`overflow_to_next` / `pages.push`).

**Correction applied (isolated, docs-only):** a `> SUPERSEDED` banner was added to
`benchmarks/runs/xfa/m1_m2_fix_runbook_risk_controls/PHASE1_M1_SAFETY_CONTRACT.md` (and a note
in that runbook's report) pointing to the M1 finding and the overflow/pagination redirect.
The runbook's M2 (flatten emit/suppression), gates, claim controls, and sequencing (M1→M2)
remain valid; only the M1 *root-cause premise* changed (instance-expansion → overflow). No
CLI code was touched by this correction.
