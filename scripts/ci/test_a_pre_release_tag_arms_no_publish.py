#!/usr/bin/env python3
"""The pre-release-tag guard, shown to bite (#358).

`a_pre_release_tag_arms_no_publish.py` exists because a grep cannot tell the
difference between a clause that blocks a publish and a clause sitting next to
an `||` that lets it through anyway. That distinction is the guard, so it is
what most of these cases are about: every one of them is a workflow that
mentions the clause and publishes a release candidate regardless.

The cases are workflow files in a temporary directory, not edits to the real
ones, so a failure here is about the guard and not about today's wasm.yml.
"""
from __future__ import annotations
import importlib.util, pathlib, subprocess, sys, tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = REPO / "scripts" / "ci" / "a_pre_release_tag_arms_no_publish.py"

spec = importlib.util.spec_from_file_location("prerelease_guard", GUARD)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)

ran = 0
fails: list[str] = []


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f" -- {detail}" if not ok and detail else ""))
    if not ok:
        fails.append(what)


def run(files: dict[str, str], *extra: str) -> subprocess.CompletedProcess:
    with tempfile.TemporaryDirectory() as td:
        wf = pathlib.Path(td) / "workflows"
        wf.mkdir()
        for name, text in files.items():
            (wf / name).write_text(text)
        return subprocess.run(
            [sys.executable, str(GUARD), "--workflows", str(wf),
             "--floor", "1", "--expect-tag-triggered", ",".join(files), *extra],
            capture_output=True, text=True)


CLAUSE = "!(github.ref_type == 'tag' && contains(github.ref_name, '-'))"


def workflow(*, job_if: str | None = None, step_if: str | None = None,
             step: str = "        run: npm publish --access public\n") -> str:
    # `if: >-` because a bare `!` starts a YAML tag: `if: !(a && b)` is not a
    # string, it is a parse error. The real workflows use the same folded form.
    text = "name: t\non:\n  push:\n    tags: ['v*.*.*']\njobs:\n  publish:\n"
    if job_if is not None:
        text += f"    if: >-\n      {job_if}\n"
    text += "    runs-on: ubuntu-latest\n    steps:\n      - name: Publish\n"
    if step_if is not None:
        text += f"        if: >-\n          {step_if}\n"
    return text + step


print("a pre-release tag reaches no publishing step")
r = run({"a.yml": workflow(step_if="startsWith(github.ref, 'refs/tags/v')")})
expect("the condition that published rc.2 is refused", r.returncode == 1, r.stderr)
expect("and it says which tag it evaluated", "v1.0.0-rc.2" in r.stderr, r.stderr)

r = run({"a.yml": workflow(
    step_if=f"{CLAUSE} && startsWith(github.ref, 'refs/tags/v')")})
expect("the clause on the step clears it", r.returncode == 0, r.stderr)

r = run({"a.yml": workflow(job_if=f"{CLAUSE} && startsWith(github.ref, 'refs/tags/v')")})
expect("the clause on the job clears its steps", r.returncode == 0, r.stderr)

# THE CASE A GREP CANNOT SEE. The text is present; the publish still happens.
r = run({"a.yml": workflow(
    step_if=f"{CLAUSE} || startsWith(github.ref, 'refs/tags/v')")})
expect("the clause behind an || does not clear it", r.returncode == 1, r.stdout + r.stderr)

r = run({"a.yml": workflow(
    step_if=f"startsWith(github.ref, 'refs/tags/v') && !contains(github.ref_name, '-')")})
expect("a differently written clause that means the same clears it",
       r.returncode == 0, r.stderr)

print("what the guard cannot know is not a pass")
r = run({"a.yml": workflow(step_if="env.PUBLISH_OK != ''")})
expect("an unknown context leaves it MAYBE, and MAYBE fails", r.returncode == 1, r.stderr)
expect("and it says MAYBE", "MAYBE" in r.stderr, r.stderr)

r = run({"a.yml": workflow(step_if="always()")})
expect("always() fails", r.returncode == 1, r.stderr)

r = run({"a.yml": workflow(step_if=f"{CLAUSE} && env.PUBLISH_OK != ''")})
expect("an unknown ANDed under the clause is still FALSE", r.returncode == 0, r.stderr)

print("the gate is inherited the way GitHub inherits it")
TWO_JOBS = f"""
name: t
on:
  push:
    tags: ['v*.*.*']
jobs:
  build:
    if: >-
      {CLAUSE} && startsWith(github.ref, 'refs/tags/v')
    runs-on: ubuntu-latest
    steps:
      - run: echo build
  publish:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - name: Publish
        run: npm publish --access public
"""
r = run({"a.yml": TWO_JOBS})
expect("a job gated through `needs:` is gated", r.returncode == 0, r.stderr)
r = run({"a.yml": TWO_JOBS.replace(f"{CLAUSE} && startsWith", "startsWith")})
expect("and an ungated dependency does not save it", r.returncode == 1, r.stderr)

# A STATUS FUNCTION REPLACES THE SUCCESS CHECK. `needs: build` with
# `if: always()` runs even when `build` was skipped, so a clause on `build`
# alone leaves the publish armed -- the guard used to report it as safely
# FALSE. (codex, #1906)
for override, gated in (("always()", False), ("cancelled() || true", False),
                        ("failure() || true", False), ("success()", True),
                        ("github.event_name == 'push'", True)):
    body = TWO_JOBS.replace("  publish:\n    needs: build\n",
                            f"  publish:\n    needs: build\n    if: >-\n      {override}\n")
    r = run({"a.yml": body})
    expect(f"a publish job with `if: {override}` is "
           + ("gated by its dependency" if gated else "NOT gated by its dependency"),
           r.returncode == (0 if gated else 1), r.stdout + r.stderr)

print("every shape of pre-release, and only those")
for tag in ("rc.2", "beta.19", "alpha.1", "0"):
    body = workflow(step_if=f"github.ref_name != 'v1.0.0-{tag}'")
    r = run({"a.yml": body})
    expect(f"a condition that only excludes v1.0.0-{tag} is not enough",
           r.returncode == 1, r.stderr)
r = run({"a.yml": workflow(step_if="!contains(github.ref_name, '-')")})
expect("the final tag still publishes", r.returncode == 0, r.stderr)
r = run({"a.yml": workflow(step_if="github.ref == 'refs/heads/master'")})
expect("a publish no tag can reach fails the final-tag floor", r.returncode == 1, r.stderr)
expect("and says so", "publishes nothing" in r.stderr or "NONE of them" in r.stderr, r.stderr)

# THE TWO WAYS A FIXED SAMPLE OF TAGS LIES. (codex, #1906)
print("the tags tried come out of the condition")
r = run({"a.yml": workflow(
    step_if="!contains(github.ref_name, '-') || startsWith(github.ref_name, 'v3.')")})
expect("a version prefix no fixture happens to use is caught",
       r.returncode == 1, r.stdout + r.stderr)
r = run({"a.yml": workflow(
    step_if="!contains(github.ref_name, '-') || endsWith(github.ref_name, '-final')")})
expect("and a suffix is too", r.returncode == 1, r.stdout + r.stderr)
r = run({"a.yml": workflow(
    step_if="!contains(github.ref_name, '-') || github.ref_name == 'v9.9.9-x'")})
expect("and a literal tag name", r.returncode == 1, r.stdout + r.stderr)

print("GitHub matches strings without regard to case")
r = run({"a.yml": workflow(
    step_if="!contains(github.ref_name, '-') || contains(github.ref_name, '-RC.')")})
expect("`-RC.` matches `v1.0.0-rc.2`, as it does on GitHub",
       r.returncode == 1, r.stdout + r.stderr)
expect("and `startsWith` is the same", guard.evaluate(
    "startsWith(github.ref_name, 'V1.')", guard.tag_env("v1.0.0-rc.2", dispatchable=True)) is True)
expect("and `==` is too", guard.evaluate(
    "github.ref_name == 'V1.0.0-RC.2'", guard.tag_env("v1.0.0-rc.2", dispatchable=True)) is True)

print("the floor: a scan that finds nothing approves everything")
NOTHING = """
name: t
on:
  push:
    tags: ['v*.*.*']
jobs:
  smoke:
    runs-on: ubuntu-latest
    steps:
      - run: cargo test
"""
r = run({"a.yml": NOTHING})
expect("no publishing step at all is FATAL, not OK", r.returncode == 2, r.stdout + r.stderr)
expect("and it says the floor was not reached", "floor" in r.stderr, r.stderr)

print("the list of workflows a tag starts")
r = run({"a.yml": workflow(step_if=f"{CLAUSE}"), "b.yml": NOTHING},
        "--expect-tag-triggered", "a.yml")
expect("a tag-triggered workflow nobody listed is reported", r.returncode == 1, r.stderr)
expect("by name", "b.yml" in r.stderr, r.stderr)
r = run({"a.yml": workflow(step_if=f"{CLAUSE}")},
        "--expect-tag-triggered", "a.yml,gone.yml")
expect("a listed workflow whose tag trigger vanished is reported",
       r.returncode == 1 and "gone.yml" in r.stderr, r.stderr)

print("what counts as publishing")
PUSH_ACTION = """        uses: docker/build-push-action@v6
        with:
          push: %s
"""
r = run({"a.yml": workflow(step="        uses: docker/build-push-action@v6\n"
                                "        with:\n          push: false\n")})
expect("a build that does not push is no publishing step (so: floor)",
       r.returncode == 2, r.stdout + r.stderr)
r = run({"a.yml": workflow(step=PUSH_ACTION % "true")})
expect("the same action with push: true is one", r.returncode == 1, r.stderr)
for signature, step in (
        ("the registry login", "        uses: docker/login-action@v3\n"),
        ("a GitHub Release", "        uses: softprops/action-gh-release@v2\n"),
        ("publish.sh", "        run: bash scripts/release/publish.sh\n"),
        ("cargo publish", "        run: cargo publish -p pdfluent\n"),
        ("twine", "        run: twine upload dist/*\n"),
        ("nuget", "        run: dotnet nuget push out/*.nupkg\n"),
        ("maven", "        run: mvn deploy -DskipTests\n")):
    r = run({"a.yml": workflow(step=step)})
    expect(f"{signature} is a publishing step", r.returncode == 1, r.stderr)

print("a condition nobody can read is not one that can be cleared")
r = run({"a.yml": workflow(step_if="startsWith(github.ref")})
expect("an unparsable condition is FATAL", r.returncode == 2, r.stdout + r.stderr)
r = run({"a.yml": workflow(step_if="${{ " + CLAUSE + " }}")})
expect("a ${{ }}-wrapped condition is read, not guessed", r.returncode == 0, r.stderr)
r = run({"a.yml": workflow(step_if="v${{ inputs.x }} == 'v1'")})
expect("a condition built by string interpolation is MAYBE", r.returncode == 1, r.stderr)

print("the evaluator itself")
PRE = guard.tag_env("v1.0.0-rc.2", dispatchable=True)
FIN = guard.tag_env("v1.0.0", dispatchable=False)
cases = [
    ("startsWith(github.ref, 'refs/tags/v')", PRE, True),
    (CLAUSE, PRE, False),
    (CLAUSE, FIN, True),
    ("contains(github.ref_name, '-')", FIN, False),
    ("inputs.publish_npm == 'true'", FIN, False),
    ("inputs.publish_npm == 'true'", PRE, None),
    ("github.ref_type == 'tag'", PRE, True),
    ("true && false", PRE, False),
    ("false || false", PRE, False),
    ("!secrets.X", PRE, None),
    ("secrets.X && false", PRE, False),
    ("secrets.X || true", PRE, True),
    ("format('refs/tags/{0}', github.ref_name) == github.ref", PRE, True),
    ("always()", PRE, True),
    ("cancelled()", PRE, None),
    ("(github.ref_type == 'tag') && !(contains(github.ref_name, '-'))", PRE, False),
]
for expression, env, want in cases:
    got = guard.evaluate(expression, env)
    expect(f"evaluate({expression!r}) is {want}", got is want, f"got {got}")

print("the status-function rule itself")
JOBS = {"build": {"if": CLAUSE}, "publish": {"needs": "build"}}
expect("without an `if:`, a job inherits its dependency's",
       guard.job_condition(JOBS, "publish") == [CLAUSE])
JOBS["publish"]["if"] = "always()"
expect("with `always()`, it does not",
       guard.job_condition(JOBS, "publish") == ["always()"])
JOBS["publish"]["if"] = "success() && github.ref_type == 'tag'"
expect("with `success()`, it does again",
       guard.job_condition(JOBS, "publish") == ["success() && github.ref_type == 'tag'", CLAUSE])

print("a branch dispatch with the publish input true arms nothing")


def dispatch_workflow(*, job_if: str) -> str:
    # Dispatch-triggered, like the npm jobs: no tag starts it, so the
    # final-tag floor (which only covers tag-triggered workflows) stays
    # out of these cases. The extra --expect-tag-triggered "" clears the
    # helper's default, which would otherwise expect a tag trigger here.
    return (
        "name: t\non:\n  workflow_dispatch:\n    inputs:\n"
        "      publish_npm:\n        default: 'false'\n"
        "      create_release:\n        default: 'false'\n"
        "jobs:\n  publish-npm:\n"
        f"    if: >-\n      {job_if}\n"
        "    runs-on: ubuntu-latest\n    steps:\n"
        "      - name: Publish\n        run: npm publish --access public\n"
    )


def run_dispatch(job_if: str):
    return run({"a.yml": dispatch_workflow(job_if=job_if)},
               "--expect-tag-triggered", "")


r = run_dispatch(f"{CLAUSE} && inputs.publish_npm == 'true'")
expect("an input-armed publish without the tag clause is refused",
       r.returncode == 1, r.stdout + r.stderr)
expect("and it says branch", "branch" in r.stderr, r.stderr)
r = run_dispatch(
    f"{CLAUSE} && github.ref_type == 'tag' && inputs.publish_npm == 'true'")
expect("with the tag clause it clears", r.returncode == 0,
       r.stdout + r.stderr)
r = run_dispatch(f"{CLAUSE} && inputs.create_release == 'true'")
expect("the create_release decision is refused the same way",
       r.returncode == 1, r.stdout + r.stderr)
r = run_dispatch(
    f"{CLAUSE} && github.ref_type == 'tag' && inputs.create_release == 'true'")
expect("and clears the same way", r.returncode == 0, r.stdout + r.stderr)
expect("branch_env is a branch",
       guard.evaluate("github.ref_type == 'tag'", guard.branch_env()) is False)
expect("branch_env carries the publish inputs true",
       guard.evaluate("inputs.publish_npm == 'true'", guard.branch_env()) is True)

print("\nand the tree it is written for")
real = subprocess.run([sys.executable, str(GUARD)], capture_output=True, text=True)
expect("today's workflows pass", real.returncode == 0, real.stdout + real.stderr)

print(f"\n{ran - len(fails)}/{ran} cases pass")
if fails:
    print("FAILED: " + "; ".join(fails), file=sys.stderr)
sys.exit(1 if fails else 0)
