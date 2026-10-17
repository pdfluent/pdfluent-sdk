#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Drives a_job_that_reaches_cargo_finds_it_on_path.py. (#516)

The guard exists for one shape, and that shape is the case a reader is most
likely to break while tidying:

    - run: bash scripts/ci/public_snapshot_audit.sh

with the compiler two files further down. So the fixture is built that way --
a workflow that names a shell script, which names a Python script, which hands
`["cargo", "metadata", ...]` to a subprocess -- and the case that matters is
that removing the `Cargo on PATH` step from THAT job turns the guard red.

The ways this guard could be green and worthless:

  it only reads the workflow                  -- then #516 walks straight past
                                                 it, because neither job says
                                                 the word cargo
  it accepts the step after the need          -- a PATH repaired at step 20 is
                                                 no use to step 3
  it judges hosted jobs too                   -- ubuntu-latest ships a
                                                 toolchain; that would be noise
  it reads `pgrep -x cargo` as an invocation  -- and then it is red about an
                                                 argument
  it lets the shared step be emptied          -- three jobs call one file now,
                                                 and a file that checks nothing
                                                 satisfies all three
"""
from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
GUARD = ROOT / "scripts" / "ci" / "a_job_that_reaches_cargo_finds_it_on_path.py"

failures: list[str] = []
done = 0


def expect(what: str, ok: bool, detail: str = "") -> None:
    global done
    done += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f"   [{detail}]" if not ok and detail else ""))
    if not ok:
        failures.append(what)


ON_PATH = ("      - name: Cargo on PATH\n"
           "        run: bash scripts/ci/cargo_on_path.sh\n")
TOOLCHAIN = ("      - uses: dtolnay/rust-toolchain@d1031067263f94b142dd6c0ce24c5eb9d02d52a0\n"
             "        with:\n"
             "          toolchain: stable\n")
# The #516 shape: nothing here says cargo.
AUDIT = ("      - name: audit the published tree\n"
         "        run: bash scripts/ci/audit.sh\n")
CACHE = "      - uses: Swatinem/rust-cache@v2\n"

# The two scripts the audit step reaches, and the compiler at the end of them.
AUDIT_SH = ('#!/usr/bin/env bash\nset -euo pipefail\n'
            'UIT="$(python3 scripts/ci/simulate.py --keep)"\nprintf "%s\\n" "$UIT"\n')
SIMULATE_PY = ('import subprocess\n'
               'STEPS = {"metadata": ["cargo", "metadata", "--no-deps"]}\n'
               'subprocess.run(STEPS["metadata"], check=False)\n')


def workflow(name: str, steps: str, runner: str = "[self-hosted, xfa-fast]") -> str:
    return (f"name: {name}\n"
            "on:\n  push:\n    branches: [master]\n"
            "jobs:\n"
            f"  {name}:\n"
            f"    runs-on: {runner}\n"
            "    steps:\n"
            "      - uses: actions/checkout@v4\n"
            f"{steps}")


def tree(files: dict[str, str], scripts: bool = True) -> tempfile.TemporaryDirectory:
    holder = tempfile.TemporaryDirectory()
    fake = pathlib.Path(holder.name)
    (fake / ".github" / "workflows").mkdir(parents=True)
    for name, body in files.items():
        (fake / ".github" / "workflows" / f"{name}.yml").write_text(body)
    if scripts:
        (fake / "scripts" / "ci").mkdir(parents=True)
        (fake / "scripts" / "ci" / "audit.sh").write_text(AUDIT_SH)
        (fake / "scripts" / "ci" / "simulate.py").write_text(SIMULATE_PY)
    return holder


def run(files: dict[str, str], *extra: str) -> subprocess.CompletedProcess:
    with tree(files) as where:
        return subprocess.run([sys.executable, str(GUARD), "--repo", where, *extra],
                              capture_output=True, text=True, timeout=120)


# Eight jobs, because the guard refuses a scan that found almost nothing.
def eight(**overrides: str) -> dict[str, str]:
    base = {f"job{n}": workflow(f"job{n}", ON_PATH + AUDIT) for n in range(1, 9)}
    base.update(overrides)
    return base


print("the compiler is two scripts down, and the guard follows it")
good = run(eight(), "--skip-shared-step")
expect("eight jobs that say where cargo is pass", good.returncode == 0, good.stdout[-400:])

bare = run(eight(job8=workflow("job8", AUDIT)), "--skip-shared-step")
expect("the same job without the step is refused", bare.returncode == 1, bare.stdout[-500:])
expect("and the job is named", "`job8`" in bare.stdout, bare.stdout[-500:])
expect("and the chain to the compiler is shown",
       "simulate.py" in bare.stdout and "cargo metadata" in bare.stdout, bare.stdout[-500:])

print("a toolchain action answers the same question")
tool = run(eight(job8=workflow("job8", TOOLCHAIN + AUDIT)), "--skip-shared-step")
expect("dtolnay/rust-toolchain counts", tool.returncode == 0, tool.stdout[-400:])

print("order is part of the rule")
late = run(eight(job8=workflow("job8", CACHE + ON_PATH)), "--skip-shared-step")
expect("a PATH repaired after the cache action is refused", late.returncode == 1,
       late.stdout[-400:])
expect("and it says which step already needed it", "rust-cache" in late.stdout,
       late.stdout[-400:])
early = run(eight(job8=workflow("job8", ON_PATH + CACHE)), "--skip-shared-step")
expect("and before it is fine", early.returncode == 0, early.stdout[-400:])

print("only the machines we own are judged")
# A ninth job rather than a replaced eighth: the floor counts the jobs on our
# own machines, and turning one of the eight hosted would trip it instead.
hosted = run(eight(job9=workflow("job9", AUDIT, runner="ubuntu-latest")), "--skip-shared-step")
expect("a hosted job needs no step of ours",
       hosted.returncode == 0 and "`job9`" not in hosted.stdout,
       f"rc={hosted.returncode}: {hosted.stdout[-400:]}")
bought = run(eight(job8=workflow("job8", AUDIT,
                                 runner="${{ needs.create-runner.outputs.label }}")),
             "--skip-shared-step")
expect("an instance a wave bought is ours", bought.returncode == 1, bought.stdout[-400:])

print("an argument is not an invocation")
prose = run(eight(job8=workflow(
    "job8",
    "      - name: say something\n"
    "        run: |\n"
    "          # cargo build would be wrong here\n"
    "          pgrep -x cargo || true\n"
    '          [ -e "$t/.cargo-lock" ] && echo busy\n')), "--skip-shared-step")
expect("a comment, a pgrep argument and a filename need no step",
       prose.returncode == 0, prose.stdout[-400:])
direct = run(eight(job8=workflow(
    "job8", "      - name: build\n        run: cargo build -p pdfluent\n")),
    "--skip-shared-step")
expect("cargo in command position is still a need", direct.returncode == 1,
       direct.stdout[-400:])

# The guard read ITSELF as a job that runs cargo, twice, because a file that
# asks "is this word one of cargo, rustc, rustup" writes those three words down.
# A membership test is not an argument list, and the distinction it now makes --
# a list is argv, a tuple or a set is a question -- is worth a case, because the
# next reader will spell it either way.
with tree(eight()) as where:
    (pathlib.Path(where) / "scripts" / "ci" / "simulate.py").write_text(
        'NAMES = ("cargo", "rustc", "rustup")\n'
        'def is_rust(word):\n    return word in NAMES\n')
    naming = subprocess.run([sys.executable, str(GUARD), "--repo", where,
                             "--skip-shared-step"],
                            capture_output=True, text=True, timeout=120)
expect("a file that only names cargo is not a file that runs it",
       naming.returncode == 0, naming.stdout[-400:])

# BOTH SIDES OF THE ARGV HEURISTIC (codex, #1875). A list that is never run is
# not an invocation, and a tuple that is run is one -- the first version of this
# guard had these the wrong way round in both directions, which would have made
# it demand a toolchain from a job that only holds a fixture, and let a script
# spelling its argv as a tuple past.
with tree(eight()) as where:
    (pathlib.Path(where) / "scripts" / "ci" / "simulate.py").write_text(
        'EXPECTED = ["cargo", "metadata", "--no-deps"]\n'
        'def check(seen):\n    return seen == EXPECTED\n')
    fixture = subprocess.run([sys.executable, str(GUARD), "--repo", where,
                              "--skip-shared-step"],
                             capture_output=True, text=True, timeout=120)
expect("a cargo argv nothing ever runs is not an invocation",
       fixture.returncode == 0, fixture.stdout[-400:])

with tree(eight(job8=workflow("job8", AUDIT))) as where:
    (pathlib.Path(where) / "scripts" / "ci" / "simulate.py").write_text(
        'import subprocess\n'
        'subprocess.run(("cargo", "metadata", "--no-deps"), check=False)\n')
    tuple_argv = subprocess.run([sys.executable, str(GUARD), "--repo", where,
                                 "--skip-shared-step"],
                                capture_output=True, text=True, timeout=120)
expect("a tuple handed to a subprocess is one", tuple_argv.returncode == 1,
       tuple_argv.stdout[-400:])

# THREE WAYS A PROVIDER CAN BE PRESENT AND USELESS (codex, #1875).
print("a provider that may be skipped is not a provider")
skippable = run(eight(job8=workflow(
    "job8",
    "      - name: Cargo on PATH\n"
    "        if: ${{ steps.check.outputs.needed == 'true' }}\n"
    "        run: bash scripts/ci/cargo_on_path.sh\n" + AUDIT)), "--skip-shared-step")
expect("a conditional provider over an unconditional need is refused",
       skippable.returncode == 1, skippable.stdout[-500:])
matched = run(eight(job8=workflow(
    "job8",
    "      - name: Cargo on PATH\n"
    "        if: ${{ steps.check.outputs.needed == 'true' }}\n"
    "        run: bash scripts/ci/cargo_on_path.sh\n"
    "      - name: audit the published tree\n"
    "        if: ${{ steps.check.outputs.needed == 'true' }}\n"
    "        run: bash scripts/ci/audit.sh\n")), "--skip-shared-step")
expect("and the same condition on both is fine", matched.returncode == 0,
       matched.stdout[-500:])
cancelled = run(eight(job8=workflow(
    "job8",
    "      - name: Cargo on PATH\n        if: ${{ !cancelled() }}\n"
    "        run: bash scripts/ci/cargo_on_path.sh\n" + AUDIT)), "--skip-shared-step")
expect("`!cancelled()` runs whenever the job does", cancelled.returncode == 0,
       cancelled.stdout[-500:])

print("printing a path is not putting it on one")
printed = run(eight(job8=workflow(
    "job8",
    "      - name: where is cargo\n        run: rustup which cargo\n" + AUDIT)),
    "--skip-shared-step")
expect("`rustup which cargo` on its own is not a provider", printed.returncode == 1,
       printed.stdout[-500:])
written = run(eight(job8=workflow(
    "job8",
    "      - name: put it on PATH\n        run: |\n"
    "          dirname \"$(rustup which cargo)\" >> \"$GITHUB_PATH\"\n" + AUDIT)),
    "--skip-shared-step")
expect("and writing the directory to $GITHUB_PATH is", written.returncode == 0,
       written.stdout[-500:])

print("the repair and the need can share a line")
same_step = run(eight(job8=workflow(
    "job8",
    "      - name: both at once\n"
    "        run: bash scripts/ci/cargo_on_path.sh && cargo metadata\n")),
    "--skip-shared-step")
expect("a cargo call after the repair in the same step is still a need",
       same_step.returncode == 1, same_step.stdout[-500:])

# A CONDITION IS AN ORDER, NOT A FLAG (codex, #1875).
print("how often a condition lets a step run is part of the rule")
looser = run(eight(job8=workflow(
    "job8",
    "      - name: Cargo on PATH\n        run: bash scripts/ci/cargo_on_path.sh\n"
    "      - name: audit the published tree\n        if: ${{ always() }}\n"
    "        run: bash scripts/ci/audit.sh\n")), "--skip-shared-step")
expect("a provider with no condition does not cover an `always()` need",
       looser.returncode == 1, looser.stdout[-500:])
stricter = run(eight(job8=workflow(
    "job8",
    "      - name: Cargo on PATH\n        if: ${{ !cancelled() }}\n"
    "        run: bash scripts/ci/cargo_on_path.sh\n"
    "      - name: audit the published tree\n"
    "        if: ${{ !cancelled() && steps.x.outputs.moved == 'true' }}\n"
    "        run: bash scripts/ci/audit.sh\n")), "--skip-shared-step")
expect("and a need that adds a term to the provider's condition is covered",
       stricter.returncode == 0, stricter.stdout[-500:])

print("every step that needs cargo is judged, not the first")
second = run(eight(job8=workflow(
    "job8",
    "      - name: Cargo on PATH\n        if: ${{ steps.x.outputs.on == 'true' }}\n"
    "        run: bash scripts/ci/cargo_on_path.sh\n"
    "      - name: first\n        if: ${{ steps.x.outputs.on == 'true' }}\n"
    "        run: bash scripts/ci/audit.sh\n"
    "      - name: second\n        run: bash scripts/ci/audit.sh\n")),
    "--skip-shared-step")
expect("a later need under another condition is still asked about",
       second.returncode == 1, second.stdout[-500:])

print("the shared step is read for what it runs")
with tree(eight()) as where:
    (pathlib.Path(where) / "scripts" / "ci" / "cargo_on_path.sh").write_text(
        "# command -v cargo\n# GITHUB_PATH\n# cargo --version\nexit 0\n")
    commented = subprocess.run([sys.executable, str(GUARD), "--repo", where],
                               capture_output=True, text=True, timeout=120)
expect("a shared step whose checks are all comments is refused",
       commented.returncode == 1, commented.stdout[-500:])

# THREE MORE WAYS TO LOOK LIKE THE ANSWER (codex, #1875).
print("a comment is not a provider either")
commented_provider = run(eight(job8=workflow(
    "job8",
    "      - name: nearly\n        run: |\n"
    "          # bash scripts/ci/cargo_on_path.sh\n"
    "          echo nothing\n" + AUDIT)), "--skip-shared-step")
expect("a commented-out repair does not count", commented_provider.returncode == 1,
       commented_provider.stdout[-500:])

print("an OR is not an AND")
either = run(eight(job8=workflow(
    "job8",
    "      - name: Cargo on PATH\n        if: ${{ steps.x.outputs.on == 'true' }}\n"
    "        run: bash scripts/ci/cargo_on_path.sh\n"
    "      - name: audit the published tree\n"
    "        if: ${{ steps.x.outputs.on == 'true' && steps.y.outputs.go == 'true' "
    "|| steps.z.outputs.forced == 'true' }}\n"
    "        run: bash scripts/ci/audit.sh\n")), "--skip-shared-step")
expect("a need that can also run through an OR branch is not covered",
       either.returncode == 1, either.stdout[-500:])

print("cargo behind env or sudo is still cargo")
wrapped = run(eight(job8=workflow(
    "job8",
    "      - name: build\n        run: env RUSTFLAGS=-Dwarnings cargo check\n")),
    "--skip-shared-step")
expect("`env NAME=value cargo check` is an invocation", wrapped.returncode == 1,
       wrapped.stdout[-500:])

# FOUR MORE SPELLINGS THAT SLIPPED THROUGH (codex, #1875).
print("the desktop can be named without the word self-hosted")
bare_label = run(eight(job9=workflow("job9", AUDIT, runner="[xfa-fast]")),
                 "--skip-shared-step")
expect("`runs-on: [xfa-fast]` is the same machine", bare_label.returncode == 1,
       bare_label.stdout[-400:])

print("naming the helper is not running it")
mentioned = run(eight(job8=workflow(
    "job8",
    "      - name: is it there\n        run: test -f scripts/ci/cargo_on_path.sh\n"
    + AUDIT)), "--skip-shared-step")
expect("`test -f cargo_on_path.sh` repairs no PATH", mentioned.returncode == 1,
       mentioned.stdout[-400:])

print("wrappers do not hide the binary")
for spelling in ("command cargo metadata", "exec cargo check", "time cargo test",
                 "sudo -u runner cargo check", "env -i cargo metadata"):
    wrapped_one = run(eight(job8=workflow(
        "job8", f"      - name: build\n        run: {spelling}\n")),
        "--skip-shared-step")
    expect(f"`{spelling}` is an invocation", wrapped_one.returncode == 1,
           wrapped_one.stdout[-300:])

print("argv may arrive by keyword")
with tree(eight(job8=workflow("job8", AUDIT))) as where:
    (pathlib.Path(where) / "scripts" / "ci" / "simulate.py").write_text(
        'import subprocess\n'
        'subprocess.run(args=["cargo", "metadata", "--no-deps"], check=False)\n')
    keyword = subprocess.run([sys.executable, str(GUARD), "--repo", where,
                              "--skip-shared-step"],
                             capture_output=True, text=True, timeout=120)
expect("`subprocess.run(args=[...])` is read too", keyword.returncode == 1,
       keyword.stdout[-400:])

# TWO MORE SPELLINGS FROM THE SAME REVIEW (codex, #1875).
print("cargo asked as a question is still cargo")
for spelling in ("if ! cargo package --list; then echo no; fi",
                 "while cargo build; do break; done"):
    with tree(eight(job8=workflow("job8", AUDIT))) as where:
        (pathlib.Path(where) / "scripts" / "ci" / "audit.sh").write_text(
            f"#!/usr/bin/env bash\n{spelling}\n")
        asked = subprocess.run([sys.executable, str(GUARD), "--repo", where,
                                "--skip-shared-step"],
                               capture_output=True, text=True, timeout=120)
    expect(f"`{spelling.split(';')[0]}` is an invocation", asked.returncode == 1,
           asked.stdout[-400:])

print("a child script named by keyword is still followed")
with tree(eight(job8=workflow("job8", AUDIT))) as where:
    scripts = pathlib.Path(where) / "scripts" / "ci"
    (scripts / "simulate.py").write_text(
        'import subprocess, sys\n'
        'subprocess.run(args=[sys.executable, "scripts/ci/inner.py"], check=False)\n')
    (scripts / "inner.py").write_text(
        'import subprocess\n'
        'subprocess.run(["cargo", "metadata", "--no-deps"], check=False)\n')
    nested = subprocess.run([sys.executable, str(GUARD), "--repo", where,
                             "--skip-shared-step"],
                            capture_output=True, text=True, timeout=120)
expect("cargo three files down, reached through `args=`", nested.returncode == 1,
       nested.stdout[-400:])

# THE TWO WAYS THIS GUARD COULD REFUSE AN HONEST JOB (codex, #1875). A guard
# that invents a need is worse than one that misses it: it blocks a push nobody
# can fix by fixing anything.
print("a sentence is not a command")
quoted = run(eight(job8=workflow(
    "job8",
    "      - name: say something\n"
    "        run: echo \"note; cargo metadata is needed here later\"\n")),
    "--skip-shared-step")
expect("a shell separator inside quotes does not start a command",
       quoted.returncode == 0, quoted.stdout[-400:])

print("an ordinary predicate is success-gated, as GitHub applies it")
ordinary = run(eight(job8=workflow(
    "job8",
    "      - name: Cargo on PATH\n        run: bash scripts/ci/cargo_on_path.sh\n"
    "      - name: audit the published tree\n"
    "        if: ${{ steps.x.outputs.needed == 'true' }}\n"
    "        run: bash scripts/ci/audit.sh\n")), "--skip-shared-step")
expect("a provider with no condition covers a plain predicate",
       ordinary.returncode == 0, ordinary.stdout[-400:])

print("a command substitution is a command, quotes or no quotes")
with tree(eight(job8=workflow("job8", AUDIT))) as where:
    (pathlib.Path(where) / "scripts" / "ci" / "audit.sh").write_text(
        '#!/usr/bin/env bash\nversion="$(cargo metadata --no-deps)"\necho "$version"\n')
    inside = subprocess.run([sys.executable, str(GUARD), "--repo", where,
                             "--skip-shared-step"],
                            capture_output=True, text=True, timeout=120)
expect("cargo inside a quoted `$(...)` is an invocation", inside.returncode == 1,
       inside.stdout[-400:])

print("`cancelled()` is not `!cancelled()`")
opposite = run(eight(job8=workflow(
    "job8",
    "      - name: Cargo on PATH\n        if: ${{ !cancelled() }}\n"
    "        run: bash scripts/ci/cargo_on_path.sh\n"
    "      - name: audit the published tree\n        if: ${{ cancelled() }}\n"
    "        run: bash scripts/ci/audit.sh\n")), "--skip-shared-step")
expect("a `!cancelled()` provider does not cover a `cancelled()` need",
       opposite.returncode == 1, opposite.stdout[-400:])

print("a step allowed to fail is not a guarantee")
tolerant = run(eight(job8=workflow(
    "job8",
    "      - name: Cargo on PATH\n        continue-on-error: true\n"
    "        run: bash scripts/ci/cargo_on_path.sh\n" + AUDIT)), "--skip-shared-step")
expect("a provider with continue-on-error does not count",
       tolerant.returncode == 1, tolerant.stdout[-400:])

print("the order the paths are read in does not hide a compiler")
with tree(eight(job8=workflow(
        "job8",
        "      - name: both routes\n        run: |\n"
        "          bash scripts/ci/long1.sh\n"
        "          bash scripts/ci/target.sh\n"))) as where:
    scripts = pathlib.Path(where) / "scripts" / "ci"
    # `target.sh` is reached at the last hop the budget allows, so its own child
    # -- the file that runs cargo -- is out of reach down that route. The direct
    # route to it, one line later, has three hops to spare.
    (scripts / "long1.sh").write_text("bash scripts/ci/long2.sh\n")
    (scripts / "long2.sh").write_text("bash scripts/ci/target.sh\n")
    (scripts / "target.sh").write_text("python3 scripts/ci/simulate.py\n")
    (scripts / "simulate.py").write_text(
        'import subprocess\nsubprocess.run(["cargo", "metadata"], check=False)\n')
    both = subprocess.run([sys.executable, str(GUARD), "--repo", where,
                           "--skip-shared-step"],
                          capture_output=True, text=True, timeout=120)
expect("a helper first seen past the hop budget is read again when reached directly",
       both.returncode == 1, both.stdout[-400:])

print("a scan that finds nothing approves nothing")
thin = run({"job1": workflow("job1", ON_PATH + AUDIT)}, "--skip-shared-step")
expect("one job is too few to be the real set", thin.returncode == 2, thin.stdout[-300:])
expect("and it says so", "approves everything" in thin.stdout, thin.stdout[-300:])
none = subprocess.run([sys.executable, str(GUARD), "--repo", "/nonexistent-tree"],
                      capture_output=True, text=True)
expect("no workflow directory is a refusal", none.returncode == 2, none.stdout[-300:])

print("the step three jobs share is checked where it lives")
with tree(eight()) as where:
    shared = pathlib.Path(where) / "scripts" / "ci" / "cargo_on_path.sh"

    def judge(body: str | None) -> subprocess.CompletedProcess:
        if body is None:
            shared.unlink(missing_ok=True)
        else:
            shared.write_text(body)
        return subprocess.run([sys.executable, str(GUARD), "--repo", where],
                              capture_output=True, text=True, timeout=120)

    whole = judge('echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"\n'
                  'command -v cargo || exit 1\ncargo --version\n')
    expect("a step that adds the directory and checks it passes",
           whole.returncode == 0, whole.stdout[-400:])
    hollow = judge('echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"\n')
    expect("a step that stopped checking is refused", hollow.returncode == 1,
           hollow.stdout[-400:])
    gone = judge(None)
    expect("and a step that is gone altogether is refused", gone.returncode == 1,
           gone.stdout[-400:])

# A RUNS-ON ROUTED THROUGH THE RUNNER_POOL INPUT IS JUDGED PER BRANCH (#636).
# The github half lands on a hosted image, which ships cargo, so only the self
# half needs a provider -- and the shared setup action is one exactly when the
# job asked it for rust.
ROUTED = "${{ inputs.runner_pool == 'self' && matrix.runs-on-self || matrix.runs-on }}"


def routed(name: str, steps: str, hosted: str = "ubuntu-latest") -> str:
    return (
        f"name: {name}\n"
        "on:\n  workflow_dispatch:\n    inputs:\n      runner_pool:\n"
        "        default: 'github'\n"
        "jobs:\n"
        f"  {name}:\n"
        f"    runs-on: {ROUTED}\n"
        "    strategy:\n      matrix:\n"
        "        runs-on-self: [self-hosted, linux, x64]\n"
        f"        runs-on: {hosted}\n"
        "    steps:\n"
        "      - uses: actions/checkout@v4\n"
        f"{steps}"
    )


SELFSETUP = (
    "      - name: Set up the self-hosted toolchain\n"
    "        if: inputs.runner_pool == 'self'\n"
    "        uses: ./.github/actions/setup-selfhosted\n"
    "        with:\n"
)
GITHUB_ONLY_AUDIT = (
    "      - name: audit the published tree\n"
    "        if: inputs.runner_pool != 'self'\n"
    "        run: bash scripts/ci/audit.sh\n"
)

# A push-triggered release workflow counts push as self: on a tag push
# inputs.runner_pool is empty, so the pool test reads push-or-self and the
# hosted-only side its negation (release-smoke.yml).
PUSHSELFSETUP = (
    "      - name: Set up the self-hosted toolchain\n"
    "        if: github.event_name == 'push' || inputs.runner_pool == 'self'\n"
    "        uses: ./.github/actions/setup-selfhosted\n"
    "        with:\n"
)
PUSHGITHUB_ONLY_AUDIT = (
    "      - name: audit the published tree\n"
    "        if: github.event_name != 'push' && inputs.runner_pool != 'self'\n"
    "        run: bash scripts/ci/audit.sh\n"
)

print("a routed job provides per branch")
split = run(
    eight(job8=routed("job8", SELFSETUP + "          tools: rust,node\n" + CACHE)),
    "--skip-shared-step",
)
expect(
    "the self-half setup covers an unconditional cache need",
    split.returncode == 0,
    split.stdout[-500:],
)
norust = run(
    eight(job8=routed("job8", SELFSETUP + "          tools: node,python\n" + AUDIT)),
    "--skip-shared-step",
)
expect(
    "a setup that was not asked for rust covers nothing",
    norust.returncode == 1,
    norust.stdout[-500:],
)
noprovider = run(eight(job8=routed("job8", AUDIT)), "--skip-shared-step")
expect(
    "a routed job with no provider at all is still refused",
    noprovider.returncode == 1,
    noprovider.stdout[-500:],
)
github_only = run(
    eight(job8=routed("job8", GITHUB_ONLY_AUDIT)),
    "--skip-shared-step",
)
expect(
    "a need that runs only on the github half is answered by the image",
    github_only.returncode == 0,
    github_only.stdout[-500:],
)
unrouted = run(
    eight(job8=routed("job8", AUDIT, hosted="[self-hosted, gpu]")),
    "--skip-shared-step",
)
expect(
    "without a hosted image named, the image is not trusted",
    unrouted.returncode == 1,
    unrouted.stdout[-500:],
)

print("a push trigger counts as the self half where the workflow says so")
pushsplit = run(
    eight(job8=routed("job8", PUSHSELFSETUP + "          tools: rust,node\n" + CACHE)),
    "--skip-shared-step",
)
expect(
    "a push-or-self setup covers an unconditional cache need",
    pushsplit.returncode == 0,
    pushsplit.stdout[-500:],
)
pushnorust = run(
    eight(job8=routed("job8", PUSHSELFSETUP + "          tools: node,python\n" + AUDIT)),
    "--skip-shared-step",
)
expect(
    "a push-or-self setup not asked for rust still covers nothing",
    pushnorust.returncode == 1,
    pushnorust.stdout[-500:],
)
pushgithub = run(
    eight(job8=routed("job8", PUSHGITHUB_ONLY_AUDIT)),
    "--skip-shared-step",
)
expect(
    "a need that runs only off-push on github is answered by the image",
    pushgithub.returncode == 0,
    pushgithub.stdout[-500:],
)

print("and this repository obeys the rule")
real = subprocess.run([sys.executable, str(GUARD)], capture_output=True, text=True, timeout=300)
expect("every job here that reaches cargo can find it", real.returncode == 0,
       real.stdout[-900:])

print(f"test_a_job_that_reaches_cargo_finds_it_on_path: {done} case(s), {len(failures)} failure(s)")
sys.exit(1 if failures else 0)
