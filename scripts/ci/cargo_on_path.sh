#!/usr/bin/env bash
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
#
# Make cargo runnable for the rest of the job, on the runner that outlives it.
# (#516, and #292 before it)
#
# WHY THIS IS A FILE AND NOT THREE COPIES OF A STEP
#
# The desktop runner is a service. A service reads no login profile, so the
# directory rustup installed cargo into is not on the PATH an Actions `run:`
# step gets, while the same command works perfectly for the person who set the
# machine up. That difference has now been discovered three times:
#
#   #292  master, 02-09-2026: a guard died on `FileNotFoundError: 'cargo'` in a
#         job whose earlier step had already added the directory.
#   #516  publication-guards.yml, every push from 09-09-2026 to 10-09-2026 --
#         38 runs, none green -- and ci.yml's `public-snapshot-build` next to
#         it, both on the same FileNotFoundError out of the same script.
#
# ci.yml carried a step that gets this right and two jobs beside it did not, so
# the step becomes a file: one place to read, one place to repair, and one thing
# for a_job_that_reaches_cargo_finds_it_on_path.py to recognise.
#
# NOT A TOOLCHAIN INSTALL. A job on a throwaway instance installs its own
# toolchain, because the image is the unknown there and that is what #495 and
# a_wave_job_that_compiles_carries_its_toolchain.py settled. Here the toolchain
# is installed, pinned by rust-toolchain.toml, and warm; the only thing missing
# is the PATH of a shell that reads no profile.
set -euo pipefail

# `$HOME` for the runner service is not necessarily where cargo was installed,
# so adding the directory is not the same as making cargo runnable. Add it,
# source the env file when it is there, and then CHECK.
if [ -n "${GITHUB_PATH:-}" ]; then
    echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"
fi
# shellcheck disable=SC1091  # written by rustup, absent on a machine without it.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env" || true
export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v cargo >/dev/null 2>&1; then
    echo "::error::cargo is not runnable on this runner. \$HOME is $HOME; \$HOME/.cargo/bin is $([ -d "$HOME/.cargo/bin" ] && echo present || echo absent). Every step below that needs the toolchain will fail with a FileNotFoundError that says nothing about this." >&2
    exit 1
fi
# Status-checked, not `echo $(…)`: if cargo resolves but cannot run -- a rustup
# shim with no toolchain installed -- the substitution fails, `echo` still
# returns 0, `set -e` sees nothing, and the step reports success with an empty
# version. The one case this check exists for would pass it. (codex, #1659)
cargo --version || {
    echo "::error::cargo is on PATH but does not run. A rustup shim without an installed toolchain does this: \`command -v cargo\` succeeds and every build fails." >&2
    exit 1
}
