# Package managers

Three package managers describe one desktop release. What each of them needs,
what is ready, and what only a person with the right account can do.

The facts they all repeat — version, download URL, size, checksum, and the
Windows Installer identifiers — live once in `desktop-release.toml`.
`scripts/ci/every_package_manifest_names_the_same_release.py` reads every
manifest back and fails on any that disagrees; it runs on every push and in
ci.yml. Bump a release by editing the record first and then the manifests, and
the guard will tell you which one you forgot.

## Homebrew — our own tap, ready to publish

`homebrew/Casks/pdfluent.rb` is a cask, not a formula. The published macOS
artefact is a `.dmg` holding a signed, notarised `PDFluent.app`; a formula
installs command-line software into a prefix and is the wrong shape for it. The
file that stood here until #192 was a formula naming an `xfa-cli` tarball that
was never released, with `sha256 "PLACEHOLDER"`, pointing at a repository that
is private. Nobody could have installed it.

The official `homebrew-cask` repository is a separate question and is answered
on the ticket: its acceptance policy sets a popularity threshold — 90 forks, 90
watchers or 225 stars for a submission by the owner — and `pdfluent/pdfluent`
is at zero. **Our own tap has no such rule.** A tap is a repository named
`homebrew-tap` under the organisation, holding `Casks/` at its root, and once it
exists anyone can run:

    brew tap pdfluent/tap
    brew install --cask pdfluent

Proven, on this cask, by `scripts/release/homebrew_cask_clean_prefix_install.sh`:
it builds a Homebrew prefix from nothing, makes the tap in it, installs, checks
the installed bundle's version, identifier, signature and Gatekeeper verdict,
uninstalls, and removes the prefix. It needs the network and is not in the
pre-push gate for that reason.

`auto_updates true` is in the cask on purpose. The application carries the Tauri
updater and replaces its own bundle, so without it `brew upgrade` and the
updater fight over the same directory.

## winget — manifest ready, submission is a decision

`winget/manifests/i/InnovationTrigger/PDFluent/1.0.0-beta.21/` — three files on
schema 1.10.0. winget sets no popularity requirement. The requirements it does
set are met: MSI is an accepted installer type, the URL is unique per version
rather than a vanity URL overwritten in place, the installer runs without
interaction, and the URL is reachable from the publisher's own site.

Read out of the MSI's own tables with `msiinfo export` (msitools), which is why
these did not need a Windows machine:

| | |
|---|---|
| ProductCode | `{AEE448FB-9AC3-4519-956E-60B7A232CDA8}` |
| UpgradeCode | `{8AE94143-984E-5CE4-893C-ED1723406728}` |
| Scope | machine — `ALLUSERS=1` stands in the Property table |
| ProductVersion | 1.0.0, and it does not move between betas |

That last row is why the manifest carries `AppsAndFeaturesEntries`. Windows
writes `1.0.0` into Apps & Features while the package version is
`1.0.0-beta.21`; without the block winget compares those two and offers the same
upgrade forever.

Silent installation: the MSI's interface is in `InstallUISequence`, which
`/quiet` does not run, and the one custom action that opens a window
(`LaunchApplication`) is conditioned on `AUTOLAUNCHAPP`, which nothing but the
finish-page checkbox sets. It does need a network connection —
`DownloadAndInvokeBootstrapper` fetches the Edge WebView2 runtime from
go.microsoft.com unless `WVRTINSTALLED` is already set.

Not submitted, and not to be submitted at `1.0.0-beta.21`: the community
repository is not a place for a pre-release. At the first stable release the
version, the URL and the checksum change and the rest of this work stands.

## Chocolatey — package ready, submission needs an account

`chocolatey/` — a nuspec and the two PowerShell scripts. It downloads the same
MSI and verifies it against the same checksum; nothing is embedded, so no
binary travels in the package.

`chocolateyuninstall.ps1` removes by ProductCode. Uninstalling by display name
removes whatever Apps & Features happens to match, and the display name is not
ours to make unique.

Submission needs a community account and goes through moderation. Like winget,
it waits for a stable release.

## Bumping a release

1. Download both artefacts, compute the checksums, and update
   `desktop-release.toml` — including `verified`, which is the day somebody
   actually downloaded them.
2. Update the four manifests. The cask interpolates `#{version}` into its URL,
   so only its `version` and `sha256` move.
3. `python3 scripts/ci/every_package_manifest_names_the_same_release.py`
4. `bash scripts/release/homebrew_cask_clean_prefix_install.sh`
5. If the MSI was rebuilt, re-read its ProductCode: a WiX major upgrade gets a
   new one each build, and a stale one uninstalls nothing.
