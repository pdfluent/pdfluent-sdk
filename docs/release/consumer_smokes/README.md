# PDFluent Consumer Smoke Tests

Gate 9 of the release gate contract. Run after publish to verify the package
is installable and importable in a clean consumer environment.

All scripts use local artefacts by default — no live registry publish required.

## Scripts

| Script | Channel | Requires |
|---|---|---|
| `smoke_rust.sh` | Rust / crates.io | `cargo`, local source or crate tarball |
| `smoke_python.sh` | Python / PyPI | `python3`, built `.whl` |
| `smoke_wasm.sh` | WASM / npm | `node`, `npm`, wasm-pack output dir |
| `smoke_dotnet.sh` | .NET / NuGet | `dotnet` CLI, built `.nupkg` |
| `smoke_java.sh` | Java / Maven | `mvn`, built `.jar` |

## Usage

```bash
# Rust (local source):
docs/release/consumer_smokes/smoke_rust.sh --local-crate crates/pdfluent

# Python (local wheel):
docs/release/consumer_smokes/smoke_python.sh \
  --wheel crates/pdf-python/target/wheels/pdfluent-1.0.0b7-*.whl

# WASM (local pkg dir):
docs/release/consumer_smokes/smoke_wasm.sh \
  --pkg-dir crates/xfa-wasm/pkg

# .NET (local nupkg):
docs/release/consumer_smokes/smoke_dotnet.sh \
  --nupkg bindings/dotnet/src/PDFluent/bin/Release/PDFluent.1.0.0-beta.6.nupkg

# Java (local jar):
docs/release/consumer_smokes/smoke_java.sh \
  --jar crates/pdf-java/target/xfa-pdf-1.0.0-beta.1.jar
```

## When to run

- After every publish step, before announcing the release.
- As part of CI on the publish branch (can run in dry-run / no-artefact mode without blocking).
- Whenever a consumer reports an install failure — run locally to reproduce.

## Failure protocol

If a smoke fails:
1. Do not announce the release.
2. Yank/retract the published version if possible.
3. Fix the issue in the source.
4. Re-run the full gate sequence from Gate 0.
5. Re-publish and re-run smoke.
