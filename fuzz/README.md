# Fuzzing

libFuzzer targets for the input-facing APIs, built with `cargo-fuzz`. The
`fuzz/` directory is its own isolated workspace (`[workspace] members = ["."]`)
so it does not perturb the main workspace's Cargo resolution.

## Prerequisites

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
```

## Build all targets (bit-rot gate)

Every declared target must compile. This is the cheapest regression check and
is what the CI `build` job runs:

```sh
cd fuzz
cargo +nightly fuzz build
```

List the targets:

```sh
cd fuzz
cargo +nightly fuzz list
```

## Run a target

```sh
cd fuzz
# Smoke: a short run to catch immediate regressions.
cargo +nightly fuzz run fuzz_pdf_parser -- -max_total_time=60 -max_len=65536

# Deep: a longer scheduled run (CI uses 120s).
cargo +nightly fuzz run fuzz_pdf_parser -- -max_total_time=120 -max_len=65536
```

Corpus and crash artifacts live under `fuzz/corpus/<target>/` and
`fuzz/artifacts/<target>/` (both git-ignored).

## Crash triage

When a target finds a crash, libFuzzer writes the reproducer to
`fuzz/artifacts/<target>/crash-<hash>`. In CI it is uploaded as the
`fuzz-*-crashes-<target>` artifact (30-day retention) and the job fails.

1. **Reproduce** deterministically:
   ```sh
   cd fuzz
   cargo +nightly fuzz run fuzz_<target> artifacts/fuzz_<target>/crash-<hash>
   ```
2. **Minimize** the reproducer to the smallest failing input:
   ```sh
   cargo +nightly fuzz tmin fuzz_<target> artifacts/fuzz_<target>/crash-<hash>
   ```
3. **File** an issue with the minimized input attached and the panic/ASAN
   backtrace. Tag the affected crate.
4. **Regress**: commit the minimized input under
   `fuzz/corpus/fuzz_<target>/` so the same crash can never silently resurface,
   and add a focused unit test in the owning crate reproducing the bug.
5. **Fix**, then confirm the reproducer no longer crashes and the unit test
   passes.

## Corpus management

```sh
cd fuzz
# Minimize a corpus (remove redundant inputs that don't add coverage).
cargo +nightly fuzz cmin fuzz_<target>
```

Seed corpora may be drawn from the repository's PDF fixtures and known
adversarial inputs (Ghostscript CVE samples, pdf.js / PDFium fuzzer corpora);
see `scripts/` corpus tooling. Keep committed seeds small.

## CI

`.gitlab-ci.yml` defines three fuzz jobs in the `fuzz_manual` stage:

- **fuzz:build** — `cargo +nightly fuzz build` over all 20 targets; auto when
  `fuzz/` or `crates/` change in an MR, manual otherwise; fails on bit-rot.
- **fuzz:smoke** — 60 s run over the seven core targets; nightly schedule
  (cron `30 2 * * *` UTC) and manual dispatch.
- **fuzz:deep** — 120 s run over all 20 targets; weekly schedule
  (cron `30 3 * * 0` UTC Sunday) and manual dispatch.

Crash artifacts are uploaded on job failure (30-day retention).
Trigger any job manually from the pipeline view (play button on the job row).

### Configuring GitLab schedules (one-time setup)

Go to **Settings → CI/CD → Schedules** in the GitLab project and create two
entries:

**Schedule 1 — nightly smoke**

| Field | Value |
|-------|-------|
| Description | `Fuzz nightly smoke` |
| Cron | `30 2 * * *` (02:30 UTC every day) |
| Target branch | `xfa/text-span-ssot` |
| Active | ✓ |

**Schedule 2 — weekly deep**

| Field | Value |
|-------|-------|
| Description | `Fuzz weekly deep` |
| Cron | `30 3 * * 0` (03:30 UTC every Sunday) |
| Target branch | `xfa/text-span-ssot` |
| Active | ✓ |

No pipeline variables are needed — both jobs trigger automatically on any
`$CI_PIPELINE_SOURCE == "schedule"` pipeline regardless of which schedule fired.
The `fuzz:build` job runs automatically on the same schedule pipelines, so
every scheduled run also validates that all 20 targets compile.

The `.github/workflows/fuzz.yml` file is retained for historical reference
but is not executed (GitHub remote is not active).
