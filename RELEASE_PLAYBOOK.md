# PDFluent SDK — Release Playbook

This playbook covers every step of a `1.0.0-beta.x` release: pre-flight,
publish, post-publish verification, and rollback/recovery.

> **Execute linearly. Do not skip steps under time pressure.**
> The order matters: crates.io dependency resolution requires each crate
> to be indexed before its dependents can be published.

---

## Version scheme

| Crate family | Scheme | Examples |
|---|---|---|
| `pdfluent` + engine crates | `1.0.0-beta.N` | beta.4, beta.5 |
| `pdf-syntax`, `pdf-interpret`, `pdf-font` | Independent semver | 0.5.1, 0.5.2 |
| Image codec forks (`pdfluent-ccitt`, etc.) | Independent semver | 0.2.0, 0.3.2 |
| `pdfluent-lopdf`, `pdfluent-cff` | Independent semver | 0.39.1 |

**Workspace version** (`Cargo.toml [workspace.package]`) controls crates
that use `version.workspace = true`. It **must be bumped** before publishing
workspace-versioned crates at a new beta level.

### ⚠️ Known gap (as of beta.4)
The workspace version is currently `1.0.0-beta.3`. Several crates manually
set `1.0.0-beta.4`. Before publishing any workspace-versioned crate at
beta.5, bump `[workspace.package] version` to `"1.0.0-beta.5"` first.

---

## Pre-release checklist

Run this checklist **before** any publish step:

```bash
# 1. Ensure on master, clean tree
git checkout master && git pull
git status                            # must be clean

# 2. Full workspace compile
cargo check --workspace --exclude pdf-desktop

# 3. Key test suites
cargo test -p pdfluent --test lifecycle --test processing_limits --test security

# 4. Release consistency check (compares local vs crates.io)
python3 scripts/check_release_consistency.py

# 5. Dry-run all publishable crates
./scripts/publish_ordered.sh          # dry-run by default

# 6. Gate CI check (must be green on master)
gh run list --branch master --limit 5
```

All six steps must pass before proceeding to publish.

---

## Publish order (topological)

Dependencies flow left→right. Publish left first.

```
pdfluent-ccitt, pdfluent-jbig2, pdfluent-jpeg2000
    └── pdf-syntax
        ├── pdf-font
        │   └── pdf-interpret
        │       └── pdf-render
        │           └── pdf-engine
        │               ├── pdf-annot
        │               ├── pdfluent-sign
        │               ├── pdfluent-forms
        │               ├── pdfluent-extract
        │               ├── pdf-compliance
        │               ├── pdf-ocr
        │               └── pdf-xfa (also needs xfa-layout-engine)
        └── xfa-dom-resolver
            ├── formcalc-interpreter
            ├── xfa-layout-engine
            └── xfa-json

pdfluent-lopdf, pdfluent-cff (forked deps, publish before pdf-syntax)
xfa-license (publish before pdfluent)
pdfluent (umbrella — publish last)
```

**Use `scripts/publish_ordered.sh --live` to execute.** It handles ordering,
wait times, and dry-run validation automatically.

---

## Publish procedure

```bash
# 1. Log in to crates.io
cargo login

# 2. Run ordered publish (dry-run by default — safe to run first)
./scripts/publish_ordered.sh

# 3. Inspect dry-run output; if clean, publish for real:
./scripts/publish_ordered.sh --live

# 4. If interrupted, resume from last failed crate:
./scripts/publish_ordered.sh --live --from pdf-engine
```

**Do not interrupt a running publish.** If interrupted mid-cascade,
see the §Recovery section below.

---

## Post-publish verification

After all crates publish:

```bash
# 1. Verify top-level crate visible on crates.io
cargo info pdfluent

# 2. Install in a fresh project
mkdir /tmp/smoke-install && cd /tmp/smoke-install
cargo init
cargo add pdfluent
cargo check
cd - && rm -rf /tmp/smoke-install

# 3. Run consistency check again (now all local = crates.io)
python3 scripts/check_release_consistency.py

# 4. Tag the release
git tag v1.0.0-beta.5        # use actual version
git push --tags

# 5. Create GitHub release
gh release create v1.0.0-beta.5 --generate-notes
```

---

## Rollback procedure

### Scenario A: Wrong version published (fixable)

If a crate was published at the wrong version **and it was not yet depended upon**
by any published crate:

```bash
# Yank the bad version (does not delete; prevents new installs)
cargo yank --version 1.0.0-beta.5 pdfluent

# Verify yank took effect
cargo info pdfluent     # should show yanked on beta.5

# Fix version in Cargo.toml and re-publish
# (requires incrementing the version — crates.io does not allow re-publishing)
```

### Scenario B: Bad metadata published (description, license, README)

```bash
# Yank the version
cargo yank --version 1.0.0-beta.5 CRATE_NAME

# Fix Cargo.toml metadata
# Bump version by patch (e.g. beta.5 → beta.5.1 is not valid semver;
# use 1.0.0-beta.5+1 or bump to beta.6)
cargo publish -p CRATE_NAME --allow-dirty
```

### Scenario C: Partial cascade (N of M crates published)

The published crates are **safe to leave** on crates.io — they are not broken,
just ahead of the rest.

```bash
# Check which crates are live
python3 scripts/check_release_consistency.py

# Resume from the first missing crate
./scripts/publish_ordered.sh --live --from LAST_MISSING_CRATE
```

### Scenario D: Crate published with wrong license

This is serious. The PDFluent Commercial License must be the `license-file`
for all pdfluent-* and pdf-* crates. If MIT or Apache-2.0 was published:

```bash
# 1. Yank immediately
cargo yank --version BAD_VERSION CRATE_NAME

# 2. Email help@crates.io explaining the error (they can advise)

# 3. Fix license-file in Cargo.toml
# 4. Bump version
# 5. Re-publish
```

### Scenario E: Crates.io index lag (dependency not found)

If a crate publish fails with "dependency not found on crates.io":

```bash
# The previous crate publish may not have propagated yet.
# Wait 60s and retry:
sleep 60
cargo publish -p FAILING_CRATE --allow-dirty
```

`scripts/publish_ordered.sh --live` automatically handles this with a 15s
wait between crates. If you encounter lag, pass `--wait 60`.

---

## Artifact validation

```bash
# Check package contents before publishing
cargo package -p pdfluent --no-verify --allow-dirty
tar -tzf target/package/pdfluent-*.crate | sort

# Must include:
#   pdfluent-*/LICENSE
#   pdfluent-*/README.md
#   pdfluent-*/src/lib.rs (or similar)
#
# Must NOT include:
#   benchmarks/  (benchmarks/)
#   .env
#   *.key
#   /tmp/ paths
```

---

## Required validation order

```
pre-flight checks
    ↓
dry-run all crates (publish_ordered.sh without --live)
    ↓
publish leaf crates (image codecs, lopdf, cff)
    ↓ wait for index propagation
publish pdf-syntax, pdf-font, pdf-interpret
    ↓ wait
publish pdf-render, xfa-dom-resolver, formcalc-interpreter
    ↓ wait
publish pdf-engine, pdf-compliance, pdf-annot, pdfluent-sign, pdfluent-forms
    ↓ wait
publish pdf-xfa, xfa-layout-engine, xfa-json, pdfluent-extract, pdf-ocr
    ↓ wait
publish xfa-license
    ↓ wait
publish pdfluent (umbrella — must be LAST)
    ↓
post-publish verification (smoke install + consistency check)
    ↓
git tag + GitHub release
```

---

## Publish guard: what NOT to publish

The following crates have `publish = false` and must **never** be published
to crates.io:

| Crate | Reason |
|-------|--------|
| `pdf-capi` | Stale MIT license; needs license fix before publish |
| `pdf-desktop` | Tauri desktop app (not an SDK crate) |
| `pdf-diff` | Internal visual comparison tool |
| `pdf-bench` | Benchmark harness (internal) |
| `pdf-node` | Native Node.js binding (shipped via npm, not crates.io) |
| `pdf-java` | Java JNI binding (shipped via Maven, not crates.io); stale MIT license |
| `pdf-python` | Python binding (shipped via PyPI/maturin, not crates.io); stale MIT license |
| `xfa-wasm` | WASM artifact (shipped via npm/wasm-pack, not crates.io) |
| `xfa-api-server` | Internal API server (not SDK) |
| `xfa-pdfrest-compare` | Oracle comparison tooling (internal) |
| `xfa-license-gen` | Internal license token generator (not part of public SDK) |
| `xfa-test-runner` | Internal corpus test runner (not part of public SDK) |

**Planned (not yet in release train — publish=true but not yet published):**

| Crate | Published name | Status |
|-------|----------------|--------|
| `pdf-invoice` | `pdf-invoice` | Planned — FDF/XFDF + e-invoicing |
| `pdf-pptx` | `pdf-pptx` | Planned — PDF → PPTX conversion |
| `pdf-xlsx` | `pdf-xlsx` | Planned — table extraction + XLSX |
| `xfa-cli` | `xfa-cli` | Planned — PDFluent CLI |

---

## Checklist for a new beta release

- [ ] Bump `[workspace.package] version` in `Cargo.toml`
- [ ] Bump individual version pins in crates that set version explicitly
- [ ] Run `python3 scripts/check_release_consistency.py`
- [ ] Run `./scripts/publish_ordered.sh` (dry-run)
- [ ] All dry-runs pass
- [ ] CI green on master
- [ ] Run `./scripts/publish_ordered.sh --live`
- [ ] Run post-publish verification
- [ ] Create git tag
- [ ] Create GitHub release
- [ ] Update `PROJECT_STATUS.md` in xfa-program-office

---

*This playbook is maintained alongside the SDK. Update it when the publish
order changes (new crates added, dependency edges change).*
