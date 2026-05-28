# CycloneDX SBOM baselines

One `*.cdx.json` per publish-eligible crate. These are the committed reference dependency graphs that
`scripts/release/sbom-generate.sh --check` compares each fresh generation against. See
`docs/release/sbom_protocol.md` for the protocol.

First-time creation (per crate):
```bash
scripts/release/sbom-generate.sh --tool-install
cp dist/sbom/<crate>.cdx.json docs/release/sbom_baselines/
git add docs/release/sbom_baselines/<crate>.cdx.json
```

Drift on subsequent runs requires either rolling back the offending dependency change or refreshing
this baseline in the same commit (with a commit message explaining the dependency reason).
