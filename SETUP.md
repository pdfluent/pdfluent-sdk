# Contributor Setup

This repository does not currently pin a toolchain with `rust-toolchain.toml`.
The commands below were checked against `cargo 1.93.1 (083ac5135 2025-12-15)`
on a local checkout.

## Prereqs

Install a current stable Rust toolchain plus the formatter and linter used in
CI:

```sh
rustup toolchain install stable
rustup default stable
rustup component add rustfmt clippy
```

Optional tools for specific parts of the workspace:

- `rustup target add wasm32-unknown-unknown` if you work on `crates/xfa-wasm`
  or other WASM targets.
- Node.js 22 if you work on `crates/pdf-desktop` or `crates/pdf-node`.
- Python 3 plus `maturin` and `pytest` if you work on `crates/pdf-python`.

## Clone + Initial Build

```sh
git clone https://github.com/pdfluent/pdfluent-sdk.git pdfluent
cd pdfluent
cargo build --workspace
```

## Tests

Run the full workspace test suite:

```sh
cargo test --workspace
```

Run one crate while iterating:

```sh
cargo test -p pdfluent
```

Match the CI lint and formatting checks before opening a PR:

```sh
cargo clippy --workspace -- -D warnings
cargo fmt --all --check
```

## Benches

The workspace includes `crates/pdf-bench`, and CI runs it with `cargo bench`:

```sh
cargo bench -p pdf-bench
```

## Platform-Specific Gotchas

- macOS: the baseline Rust workspace build does not require extra Homebrew
  packages in CI.
- Linux: the baseline workspace builds in CI on `ubuntu-latest` with the Rust
  toolchain. Ops and benchmark scripts such as `scripts/vps-setup.sh` install
  additional packages like `build-essential`, `pkg-config`, `libssl-dev`,
  `sqlite3`, `default-jre`, `imagemagick`, and `poppler-utils`; only install
  those if you use those flows.
