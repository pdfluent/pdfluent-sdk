# CycloneDX SBOM baselines

One `*.cdx.json` per publish-eligible crate. These are the committed reference dependency graphs that
`scripts/release/sbom-generate.sh --check` compares each fresh generation against. See
`docs/release/sbom_protocol.md` for the protocol.

**Current state:** 36 baselines, regenerated 2026-10-16 for the 1.1.0 release candidate N2 (on top
of `2c53596a6`): `pdfluent-lopdf` 0.39.6, `pdf-syntax`/`pdf-interpret` 0.6.0, and the `pdf-ocr` and
`epub-extract` edges the extraction crates and the CLI gained since 2026-10-02. Generator: `cargo-cyclonedx 0.5.7` with `--target all` (target-
independent dep set so the baseline is the same on macOS-aarch64 and CI's Linux-x86_64). All
`bom-ref`/`dependencies` paths are canonicalised to `path://<crate>#<rest>` so the baselines are
machine-independent and contain zero local filesystem paths. `xfa-cli` has no baseline (it is
`publish = false` now) and neither does the deleted `xfa-license` crate; `pdfluent-cli`,
`epub-extract`, `pptx-extract`, `xlsx-extract`, `pdf-standard-fonts` and `pdf-substitute-fonts`
are newly covered.

Refreshing a baseline (e.g. an intentional dependency bump):
```bash
scripts/release/sbom-generate.sh                       # writes to dist/sbom/
cp dist/sbom/<crate>.cdx.json docs/release/sbom_baselines/
git add docs/release/sbom_baselines/<crate>.cdx.json
# commit message must explain WHY (which dep changed and the trigger)
```

Drift on subsequent runs requires either rolling back the offending dependency change or refreshing
this baseline in the same commit (with a commit message explaining the dependency reason).

Cleanliness invariants (enforced by reviewers; spot-check with the commands below before committing):
- `grep -l '/Users\|/home/[a-zA-Z]\|/var/cache' docs/release/sbom_baselines/*.cdx.json` → empty
- `grep -l '%PDF-' docs/release/sbom_baselines/*.cdx.json` → empty (no PDF binary leaks)
- `grep -l 'token\|secret\|password' docs/release/sbom_baselines/*.cdx.json` → empty
