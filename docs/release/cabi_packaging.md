# C ABI Packaging

This document covers how the PDFluent C ABI shared library is packaged for
distribution, what the produced tarball contains, how consumers integrate it,
and how the release audit verifies the artefact.

## TL;DR

```bash
bash scripts/release/package_cabi.sh
# → target/release/pdfluent-capi-<version>.tar.gz
```

Then audit:

```bash
bash scripts/release/audit-all-packages.sh --channel cabi
```

## What the script does

`scripts/release/package_cabi.sh` performs the following steps:

1. Reads the version from `crates/pdf-capi/Cargo.toml`.
2. Runs `cargo build -p pdf-capi --release` (skip with `--skip-build`).
3. Stages a `pdfluent-capi-<version>/` directory with:
   - `include/` — public C headers (`pdfluent.h`, `pdf_engine.h`).
   - `lib/` — built shared library (`cdylib`: `.dylib`/`.so`/`.dll`)
     for the host triple. The static archive is intentionally **not**
     bundled — see "Why no staticlib" below.
   - `LICENSE` — the PDFluent Commercial License text. Sourced from
     `crates/pdf-capi/LICENSE`.
   - `README.md` — consumer-facing build and integration notes. Sourced
     from `crates/pdf-capi/README.md`.
   - `VERSION` — plain text version string (matches `Cargo.toml`).
4. Creates `target/release/pdfluent-capi-<version>.tar.gz`.
5. Computes and prints a SHA-256.
6. Lists the tarball contents in the summary.

The script is host-specific: it bundles whichever libraries the local cargo
target dir contains. Cross-platform release tarballs are produced by running
the script on each target host (mac arm64, mac x86_64, linux x86_64, linux
arm64, windows x86_64).

## Tarball layout

```
pdfluent-capi-<version>/
  include/
    pdfluent.h
    pdf_engine.h
  lib/
    libpdf_capi.dylib   # macOS only
    libpdf_capi.so      # linux only
    pdf_capi.dll        # windows only
  LICENSE
  README.md
  VERSION
```

## Consumer integration

Linkers expect the header dir + lib dir:

```bash
gcc your_code.c \
  -I pdfluent-capi-<version>/include \
  -L pdfluent-capi-<version>/lib \
  -lpdf_capi \
  -o your_binary
```

On macOS/linux the dynamic library must be on the loader path at runtime
(`DYLD_LIBRARY_PATH`/`LD_LIBRARY_PATH`) or RPATH-stamped into the binary.

## Audit rules

The C ABI channel of `audit-all-packages.sh` checks:

| Rule | Severity | Notes |
|------|----------|-------|
| Tarball exists at `target/release/pdfluent-capi-*.tar.gz` | P0 | If absent, channel reports `NO_ARTIFACT` and does not fail. |
| Tarball extracts to a single root dir `pdfluent-capi-<version>/` | P0 | Required for predictable consumer layout. |
| `VERSION` file present and matches `crates/pdf-capi/Cargo.toml` `version` | P0 | Prevents stale version drift. |
| `include/` contains `*.h` files | P0 | Headers are the contract; package is unusable without them. |
| `lib/` is non-empty | P0 | At least one platform library must be present. |
| `LICENSE` present and contains "PDFluent Commercial License" | P0 | Required by commercial distribution policy. |
| `README.md` present | P0 | Consumer guidance is required. |
| No private paths, GitHub URLs, or SBOM leakage | P0 | Enforced by `audit_package_tree.py`. |

Run only the C ABI channel:

```bash
bash scripts/release/audit-all-packages.sh --channel cabi
```

## Why no staticlib

The `pdf-capi` crate produces both a `cdylib` and a `staticlib` (see
`[lib] crate-type` in `Cargo.toml`). The static archive is excluded from
the distribution tarball for two reasons:

1. **Source-path string literals.** Some upstream crates embed resource
   paths via `concat!(env!("CARGO_MANIFEST_DIR"), …)` patterns. These
   become string literals in the staticlib that `--remap-path-prefix`
   cannot rewrite (the macro expands before the remap is applied).
2. **Precompiled `compiler_builtins`.** Rust's prebuilt standard library
   distributes `compiler_builtins` with `/Users/runner/...` paths from
   the rustc CI build host. These survive in the staticlib but are not
   linked into the cdylib's exported symbols.

The cdylib is verified leakage-free under the standard audit. Consumers
who require static linking should build from source.

## Why no `cargo publish`

`crates/pdf-capi/Cargo.toml` carries `publish = false`. The crate is not
suitable for crates.io: it ships a `cdylib` that consumers do not build
through cargo. The standalone tarball is the canonical distribution
channel for C ABI consumers.

## Maintenance checklist

When changing the C ABI:

- [ ] Bump `version` in `crates/pdf-capi/Cargo.toml`.
- [ ] Update headers in `crates/pdf-capi/include/`.
- [ ] Update `crates/pdf-capi/README.md` if the consumer surface changed.
- [ ] Re-run `bash scripts/release/package_cabi.sh`.
- [ ] Re-run `bash scripts/release/audit-all-packages.sh --channel cabi`.
- [ ] Publish (manual upload to release artefact host).
