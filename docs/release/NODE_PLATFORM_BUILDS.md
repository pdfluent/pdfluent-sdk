# Building the six @pdfluent/node platform packages

Written 23-08-2026, while preparing the 1.0.0 publish. The release runbook says
"build on the correct release runner", which reads as "you need six machines".
You need one, plus the Windows box.

## What builds where

| npm package | rust target | from a Mac? |
|---|---|---|
| `@pdfluent/node-darwin-arm64` | `aarch64-apple-darwin` | yes, native |
| `@pdfluent/node-darwin-x64` | `x86_64-apple-darwin` | yes, native cross |
| `@pdfluent/node-linux-arm64-gnu` | `aarch64-unknown-linux-gnu` | yes, via `cargo zigbuild` |
| `@pdfluent/node-linux-x64-gnu` | `x86_64-unknown-linux-gnu` | yes, via `cargo zigbuild` |
| `@pdfluent/node-linux-x64-musl` | `x86_64-unknown-linux-musl` | yes, with the flag below |
| `@pdfluent/node-win32-x64-msvc` | `x86_64-pc-windows-msvc` | **no** — see below |

## The two things that are not obvious

**Linux needs `cargo zigbuild`, not `cargo build`.** A plain
`cargo build --target x86_64-unknown-linux-gnu` fails on the C dependencies
(`rquickjs-sys`) with `failed to find tool "x86_64-linux-gnu-gcc"`. Having the
Rust target installed is not the same as being able to link. `zig` supplies the
C toolchain; both `zig` and `cargo-zigbuild` are already on the release Mac.

**musl needs the static CRT switched off**, or it silently produces only an
`rlib` and no `.so`:

```bash
RUSTFLAGS="-C target-feature=-crt-static" \
  cargo zigbuild --release -p pdf-node --target x86_64-unknown-linux-musl
```

`crate-type` is already `["cdylib", "rlib"]`. musl defaults to static linking,
and a cdylib cannot come out of a static CRT — so the build *succeeds*, writes
an rlib, and the packaging step later finds no shared object. It looks like a
packaging bug and is a linker default.

**Windows is the one that needs another machine.** `cargo zigbuild` cannot
produce `x86_64-pc-windows-msvc` for crates with C code; the MSVC libraries are
not something zig substitutes. Build it on the LAN box (`a build machine`,
passwordless SSH) — the same machine that builds the editor's `.msi`.

## Sizes measured on 23-08 (release, before strip)

| target | bytes |
|---|---|
| aarch64-apple-darwin | 11,523,216 |
| x86_64-apple-darwin | 44,847,312 |
| aarch64-unknown-linux-gnu | 15,309,416 |
| x86_64-unknown-linux-gnu | 21,750,816 |
| x86_64-unknown-linux-musl | 21,761,992 |

The macOS x64 artefact is four times the arm64 one. Not investigated; noted here
so nobody mistakes it for a corrupted build.
