# Building the seven @pdfluent/node platform packages

Written 23-08-2026, while preparing the 1.0.0 publish; `linux-arm64-musl`
joined for 1.1.0. The release runbook says "build on the correct release
runner", which reads as "you need seven machines". You need one, plus the
Windows box.

## What builds where

| npm package | rust target | from a Mac? |
|---|---|---|
| `@pdfluent/node-darwin-arm64` | `aarch64-apple-darwin` | yes, native |
| `@pdfluent/node-darwin-x64` | `x86_64-apple-darwin` | yes, native cross |
| `@pdfluent/node-linux-arm64-gnu` | `aarch64-unknown-linux-gnu` | yes, via `cargo zigbuild` |
| `@pdfluent/node-linux-arm64-musl` | `aarch64-unknown-linux-musl` | yes, with the flag below |
| `@pdfluent/node-linux-x64-gnu` | `x86_64-unknown-linux-gnu` | yes, via `cargo zigbuild` |
| `@pdfluent/node-linux-x64-musl` | `x86_64-unknown-linux-musl` | yes, with the flag below |
| `@pdfluent/node-win32-x64-msvc` | `x86_64-pc-windows-msvc` | **no** — see below |

```bash
npx napi build --target aarch64-unknown-linux-musl --release
```

for the new one; the musl CRT flag below covers it like every musl target.

## The two things that are not obvious

**Linux needs `cargo zigbuild`, not `cargo build`.** A plain
`cargo build --target x86_64-unknown-linux-gnu` fails on the C dependencies
(`rquickjs-sys`) with `failed to find tool "x86_64-linux-gnu-gcc"`. Having the
Rust target installed is not the same as being able to link. `zig` supplies the
C toolchain; both `zig` and `cargo-zigbuild` are already on the release Mac.

**musl needs the static CRT switched off**, or it silently produces only an
`rlib` and no `.so`:

```bash
cargo zigbuild --release -p pdf-node --target x86_64-unknown-linux-musl
```

`crate-type` is already `["cdylib", "rlib"]`. musl defaults to static linking,
and a cdylib cannot come out of a static CRT — so the build *succeeds*, writes
an rlib, and the packaging step later finds no shared object. It looks like a
packaging bug and is a linker default. The `-C target-feature=-crt-static`
that fixes it is exported by `scripts/release/musl_cdylib_rustflags.sh` —
source it in a subshell around the musl build. It must NOT live in
`.cargo/config.toml`: that file cannot tell a cdylib from a binary, and with
the flag on the whole musl target the musl CLI came out dynamically linked.
The helper repeats the path scrubbing beside the flag, because env `RUSTFLAGS`
replace the config's flags outright.

**Windows is the one that needs another machine.** `cargo zigbuild` cannot
produce `x86_64-pc-windows-msvc` for crates with C code; the MSVC libraries are
not something zig substitutes. Build it on the Windows LAN box (passwordless
SSH; its hostname is in the private infrastructure notes, not here — this file
goes public with the tree under #222) — the same machine that builds the
editor's `.msi`.

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
