# Channel checklist — crates.io

Governed by `docs/release/PUBLISH_PROTOCOL.md`. Every box must be checked **and** recorded in the audit report under `benchmarks/runs/prepublish_audits/<crate>-<version>.md` before `cargo publish`.

## Prepublish audit

- [ ] `git status --porcelain` empty (working tree clean).
- [ ] All publish-bound commits are on the publish branch.
- [ ] `Cargo.toml` `[package].license` or `[package].license-file` correctly set.
- [ ] `[package].description`, `repository`, `homepage`, `documentation`, `readme` are populated.
- [ ] `[package].publish` is not `false` (and not `[]`) unless intentionally internal.
- [ ] Crate version is **new** — not yet published on crates.io.
- [ ] If forked open-source: confirm dual-licence files (`LICENSE-APACHE` + `LICENSE-MIT`) exist in the crate directory.
- [ ] If proprietary: confirm `LICENSE` exists with the full PDFluent Commercial Licence text and matches the canonical sha256.
- [ ] If Apache-2.0-derived: `NOTICE` exists if upstream ships one.

## Package build command

```
cargo package -p <crate> --list
```

Save the file list as the package-list snapshot for the audit report.

```
cargo package -p <crate>
```

Locate the produced `.crate` under `target/package/<crate>-<version>.crate`.

## Package content inspection

- [ ] Unpack `.crate` into a temp directory: `tar -xzf <crate>-<version>.crate -C /tmp/audit-<crate>-<version>/`.
- [ ] Verify the top-level directory in the tarball is `<crate>-<version>/`.
- [ ] Verify `Cargo.toml`, `Cargo.toml.orig`, `LICENSE` (or licence dual files), `README.md` are present.
- [ ] Run the leakage scan from `scripts/release/audit_package_tree.py` against the unpacked directory.
- [ ] Confirm package compressed size ≤ 50 MiB (or document waiver).

## Licence verification

- [ ] Manifest declares licence in `[package].license` or `[package].license-file`.
- [ ] Actual licence text file(s) present **inside the unpacked tarball** at the expected path.
- [ ] Licence file sha256 recorded in audit report.
- [ ] For `MIT OR Apache-2.0`: both `LICENSE-MIT` and `LICENSE-APACHE` are present.
- [ ] `NOTICE` present if required.

## Dependency check

- [ ] All direct deps resolve through the registry (cargo dry-run succeeds).
- [ ] No `=`-pinned dep resolves to a yanked version through any required feature.
- [ ] Optional / dev-only `=`-pinned deps that resolve to yanked versions are explicitly noted in the audit report with the rationale why they're unreachable.
- [ ] No `publish = false` workspace member ends up in the dependency closure.

## Dry-run command

```
cargo publish -p <crate> --dry-run
```

Must succeed unless the failure is a documented predecessor-not-yet-published propagation case (in which case the propagation order is recorded in the audit report).

## Publish command

```
cargo publish -p <crate>
```

Run only after `--dry-run` passes and the audit report is committed. **Never** use `--allow-dirty`.

## Post-publish verification

- [ ] crates.io API `/api/v1/crates/<name>` reports the new version as `newest_version` (allow up to a minute for index propagation).
- [ ] Download `https://static.crates.io/crates/<name>/<name>-<version>.crate` and compare its size + sha256 with the local `target/package/<name>-<version>.crate`.
- [ ] Re-run `tar -xzOf <downloaded>.crate <crate>-<version>/LICENSE` (and `LICENSE-APACHE` / `LICENSE-MIT` / `NOTICE` as applicable) and verify the licence text.
- [ ] Run a clean `cargo new /tmp/smoke-<crate> && cd /tmp/smoke-<crate> && cargo add <crate>@<version> && cargo check` and confirm the consumer resolves.
- [ ] Add a one-line entry to `benchmarks/runs/prepublish_audits/<crate>-<version>.md` recording the verification PASS.

## Rollback / yank / remediation

- Yank only when the published artifact violates the protocol (e.g. missing licence file, leaked secret).
- Open a remediation report under `benchmarks/runs/remediation/<incident>.md`.
- Prepare the fixed version. Re-run the full prepublish audit on the fix.
- Republish the fix.
- Update this checklist if the audit script should now catch the defect class.

## Failure modes specific to crates.io

- **Transitive `=`-pinned yanked dep**. Republish the broken intermediate crate as a new version with an updated pin before publishing the new consumer.
- **Optional dep that activates a yanked feature path**. Either (a) remove the activation in the consumer, or (b) republish the dependency with non-yanked pins.
- **Version already exists**. crates.io versions are immutable. Pick a new version number; do not "reuse" by yanking the old one to free the slot — the slot stays taken.
