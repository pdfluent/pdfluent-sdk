# The `pdfluent` CLI as a sixth channel (T6)

The CLI itself belongs to its own milestone. This runbook covers only how it
ships: the seven release archives, the Homebrew tap, `cargo install`,
the ghcr image, and the gates and smokes around them.

Nothing is published automatically. Every publish below is a manual job the
owner hears about beforehand.

## 0. Preconditions

- The crate is `pdfluent-cli`; the command is `pdfluent` (`cargo install
  pdfluent-cli` installs the command `pdfluent`). `cli_build_archives.sh`
  stages a prebuilt `pdfluent` binary per triple and refuses one still
  named `pdfluent-cli`, which the gate would reject.
- The `extract` subcommand (`extract INPUT --format markdown`) exists. Until
  then `smoke_cli.sh` skips in a dev lane and fails in a release lane.

## 1. Release artefacts: the seven platforms, one table

The same seven platforms as the other channels — one table in
`scripts/release/artifacts_cover_every_platform.py`, so a platform that is
added is required everywhere.

| platform | artefact |
|---|---|
| macOS arm64 | `pdfluent-<v>-aarch64-apple-darwin.tar.gz` |
| macOS x64 | `pdfluent-<v>-x86_64-apple-darwin.tar.gz` |
| Linux x64 (glibc) | `pdfluent-<v>-x86_64-unknown-linux-gnu.tar.gz` |
| Linux arm64 (glibc) | `pdfluent-<v>-aarch64-unknown-linux-gnu.tar.gz` |
| Linux x64 (musl, static) | `pdfluent-<v>-x86_64-unknown-linux-musl.tar.gz` |
| Linux arm64 (musl, static) | `pdfluent-<v>-aarch64-unknown-linux-musl.tar.gz` |
| Windows x64 | `pdfluent-<v>-x86_64-pc-windows-msvc.zip` |

Every archive contains `pdfluent[.exe]`, `LICENSE`, `LICENSE-COMMERCIAL`,
`NOTICE`, `THIRD_PARTY_LICENSES.txt`, `README.md` and the man pages under
`man/man1/`. Next to them: `SHA256SUMS`.

Build hosts: build each owned target first, then stage one `--binary`
per `--triple`, in order (the default triples are the four Linux rows):

```bash
# Linux targets (here or on the lane machine, never the Mac for Linux):
for t in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu \
         x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do
  cargo zigbuild -p pdfluent-cli --release --target "$t"
done
scripts/release/cli_build_archives.sh --version <v> --out dist/cli \
  --binary target/x86_64-unknown-linux-gnu/release/pdfluent \
  --binary target/aarch64-unknown-linux-gnu/release/pdfluent \
  --binary target/x86_64-unknown-linux-musl/release/pdfluent \
  --binary target/aarch64-unknown-linux-musl/release/pdfluent

# macOS targets, on the operator's Mac:
cargo build -p pdfluent-cli --release --target aarch64-apple-darwin
cargo build -p pdfluent-cli --release --target x86_64-apple-darwin
scripts/release/cli_build_archives.sh --version <v> --out dist/cli \
    --triple aarch64-apple-darwin --triple x86_64-apple-darwin \
    --binary target/aarch64-apple-darwin/release/pdfluent \
    --binary target/x86_64-apple-darwin/release/pdfluent

# Windows target, on the Windows host:
cargo build -p pdfluent-cli --release --target x86_64-pc-windows-msvc
scripts/release/cli_build_archives.sh --version <v> --out dist/cli \
    --triple x86_64-pc-windows-msvc \
    --binary target/x86_64-pc-windows-msvc/release/pdfluent.exe
```

The Linux targets build with `cargo zigbuild`, the musl ones statically
linked. The gate runs when all seven archives are staged in `--out`.

## 2. Homebrew tap

- Location: repo `pdfluent/homebrew-tap` (creating it is a manual,
  account-visible step for the owner), formula `Formula/pdfluent.rb`.
- The formula points `on_macos`/`on_linux` x `on_arm`/`on_intel` at the four
  macOS and glibc archives above, with their sha256 from `SHA256SUMS`, plus
  a `--version` test and an extract on a fixture the formula brings along.
- It is generated, never edited by hand:

```bash
python3 scripts/release/render_homebrew_formula.py --version <v> \
    --sums dist/cli/SHA256SUMS --out Formula/pdfluent.rb
python3 scripts/release/render_homebrew_formula.py --version <v> \
    --sums dist/cli/SHA256SUMS --out Formula/pdfluent.rb --brew-audit
```

- Pushing it to the tap is manual. `brew install --build-from-source` is not
  needed (binaries only); `brew audit --strict` runs in the gate via
  `--brew-audit` on the operator Mac.

## 3. `cargo install pdfluent-cli`

The precondition that bites: every dependency must be on crates.io at the
right version. The forked crates drift away from what is published, and only
a real `cargo publish` shows it.

```bash
python3 scripts/release/cli_cratesio_gate.py --require-clean
```

runs `cargo publish -p pdfluent-cli --dry-run` plus `cargo package --list`
and fails naming every direct path dependency without a version. Until the
release commit flips `publish = false`, the gate stays red with that
precondition named.

Smoke: `cargo install --path` in a clean `rust:1-slim` container, then
`pdfluent extract` on the fixture. After the publish, via
`cargo install pdfluent-cli --version <v>`.

## 4. Docker image on ghcr

- Image: `ghcr.io/pdfluent/pdfluent:<v>` and `:latest`, multi-arch
  `linux/amd64` + `linux/arm64`.
- Base: `alpine:3.20` with the static musl binary. `ENTRYPOINT ["pdfluent"]`;
  `WORKDIR /data`; licence files in `/usr/share/doc/pdfluent/`.
- Built from the musl archives of §1, not compiled in the Dockerfile, so the
  image holds exactly the binaries the gate checked:

```bash
scripts/release/cli_docker_build.sh --version <v> --archives dist/cli
```

- Pushing it to ghcr is manual and account-visible (`--push`, by hand).
- Smoke, on amd64 and on arm64:

```bash
docker run --rm -v $PWD/fixtures:/data ghcr.io/pdfluent/pdfluent:<v> \
    extract /data/structured.pdf --format markdown
```

or the same through `smoke_cli.sh --docker-image`.

## 5. Artefact gate

```bash
python3 scripts/release/artifacts_cover_every_platform.py \
    --cli-dir dist/cli --cli-version <v>
python3 scripts/release/artifacts_cover_every_platform.py \
    --formula Formula/pdfluent.rb --sums dist/cli/SHA256SUMS --cli-version <v>
docker buildx imagetools inspect ghcr.io/pdfluent/pdfluent:<v> --raw > /tmp/manifest.json
python3 scripts/release/artifacts_cover_every_platform.py \
    --docker-manifest /tmp/manifest.json
```

- CLI channel: all seven archives present, each with `pdfluent[.exe]` of the
  right architecture (`file`), the four licence and notice files plus the
  readme, and a size floor per archive. Both musl builds statically linked
  (`ldd` says "not a dynamic executable").
- Homebrew: the formula names all four archives with a sha256 equal to
  SHA256SUMS.
- Docker: the manifest list holds exactly `linux/amd64` and `linux/arm64`.
- Each check fails naming the platform; a fixture with one platform removed
  turns it red.

## 6. Smoke per platform

On the platform itself (GCP arm/x86 VMs and Alpine containers, macOS/Windows
on their own hosts):

```bash
scripts/release/consumer_smokes/smoke_cli.sh --artifact dist/cli/pdfluent-<v>-<triple>.tar.gz
```

Unpacks the archive into a clean directory, runs `pdfluent --version` and
`pdfluent extract <fixture> --format markdown`. The output must contain the
known sentence; exit 0 is not enough on its own. It runs without a licence
key. `--format json` and `--format chunks` run when `--help` advertises
them, so help and smoke cannot drift apart.

Post-publish: download the published archives, run the Homebrew install from
the tap, `cargo install` from crates.io and `docker pull` from ghcr, then
the same smoke (`--bin` for the installs, `--docker-image` for the image).
The registry re-read is automated:

```bash
scripts/release/post_publish_contents_check.sh --channel cli --version <v>
```

## 7. Order

The CLI channel ships after the CLI milestone, in the same release candidate
as the other channels. The RC is only "green" when all six channels pass
the artefact gate and all smokes.
