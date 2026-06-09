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

# Deep: a longer scheduled run (CI uses 300s).
cargo +nightly fuzz run fuzz_pdf_parser -- -max_total_time=300 -max_len=65536
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

`.github/workflows/fuzz.yml` defines three jobs:

- **build** — `cargo +nightly fuzz build` on every trigger; fails on bit-rot.
- **smoke** — short run (60s) over the seven core targets; runs on the nightly
  schedule and on manual dispatch.
- **deep** — full run (300s, configurable) over all targets; runs on the weekly
  schedule and on manual dispatch.

Trigger manually from the Actions tab (`workflow_dispatch`), optionally pinning
a single `target` and `duration`.
