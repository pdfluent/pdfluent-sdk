# Channel checklist — WASM packages

Governed by `docs/release/PUBLISH_PROTOCOL.md`. WASM artifacts are usually distributed via npm; if so, the **npm checklist also applies in full** — this one adds WASM-specific checks.

## Prepublish audit

- [ ] `git status --porcelain` empty.
- [ ] `wasm-pack build --release` (or `cargo build --target wasm32-unknown-unknown --release`) succeeds from a clean target dir.
- [ ] The Rust crate that produces the `.wasm` has the correct `Cargo.toml.license` field.
- [ ] The npm wrapper (e.g. `pkg/package.json`) has `name`, `version`, `description`, `license`, `repository`, `files`.
- [ ] WASM-bindgen / wasm-pack generated files (`*_bg.wasm`, `*.js`, `*.d.ts`) all carry the expected version.

## Package build commands

```
wasm-pack build --release --target bundler --out-dir pkg
```

or

```
wasm-pack build --release --target nodejs --out-dir pkg-node
```

depending on the consumer target. For multiple targets, audit each output.

## Package content inspection

- [ ] `npm pack pkg/` (and equivalents for additional targets) produces a `.tgz`.
- [ ] Extract and verify the following inside the tarball: `package.json`, `<name>_bg.wasm`, `<name>_bg.wasm.d.ts`, `<name>.js`, `<name>.d.ts`, `LICENSE`, `README.md`.
- [ ] Run `scripts/release/audit_package_tree.py` against the unpacked tarball.
- [ ] Size sanity: WASM artifacts often run 1–10 MiB; flag if > 50 MiB and document why.
- [ ] No source `.rs` files should ship unless explicitly desired (they usually shouldn't).

## Licence verification

- [ ] `LICENSE` (or dual files) inside the WASM npm tarball.
- [ ] `package.json.license` matches the file.
- [ ] If the `.wasm` includes statically-linked third-party code (libpng, freetype, etc.), include their licences inside a `licenses/` directory in the tarball — verify presence.
- [ ] Licence sha256(s) recorded in the audit report.

## WASM-specific binary checks

- [ ] `wasm-objdump -h <name>_bg.wasm` runs without error.
- [ ] No `data` section contains absolute workstation paths. Run `strings <name>_bg.wasm | grep -E '/Users/|/home/|/opt/xfa|/mnt/'` — must be empty.
- [ ] Compare gzipped size to last published version; flag ≥ 2× growth as a stop-the-line trigger.

## Dependency check

- [ ] `npm ls --omit=dev` clean (since WASM packages are usually consumed via npm).
- [ ] Rust crate side: `cargo tree -p <crate>` shows no yanked deps in any feature path activated by the WASM build.

## Dry-run command

```
npm publish --dry-run --access public pkg/
```

For the underlying Rust crate (when also published to crates.io), run `cargo publish --dry-run -p <crate>` separately as per the crates.io checklist.

## Publish command

```
npm publish --access public pkg/
```

If publishing to a private GitLab npm registry, configure `publishConfig.registry` in `pkg/package.json`.

## Post-publish verification

- [ ] `npm view <pkg> version` reports the new version.
- [ ] Download the tarball from the registry CDN and re-extract.
- [ ] Verify the `.wasm` bytes match the locally-built `.wasm` (sha256).
- [ ] Run a clean consumer smoke: `mkdir /tmp/smoke-<pkg> && cd /tmp/smoke-<pkg> && npm init -y && npm install <pkg>@<version>` and write a tiny `index.mjs` that imports and initialises the module.
- [ ] Record PASS in the audit report.

## Rollback / yank / remediation

- Same as npm — `unpublish` within 72 hours, `deprecate` after.
- For a `.wasm` binary that leaked workstation paths or contained unintended symbols, deprecate the version and ship a stripped fixed version.
- Update the audit script to scan for the offending pattern.

## Failure modes specific to WASM

- **Debug symbols leak**. A release build that accidentally kept debug symbols can carry `/Users/<name>/…/src/foo.rs` strings inside the `.wasm`. Always run the strings + path-leakage scan.
- **Two npm wrappers (bundler + nodejs)**. If you publish two targets, audit both; one passing does not validate the other.
- **Outdated wasm-pack**. Older `wasm-pack` versions emit different file names; the audit script must locate `_bg.wasm` by glob, not by hard-coded filename.
