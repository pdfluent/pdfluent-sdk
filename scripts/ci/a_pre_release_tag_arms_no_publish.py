#!/usr/bin/env python3
"""A pre-release tag must arm no publish job (#358).

WHAT HAPPENED. On 10-09-2026 `v1.0.0-rc.2` was pushed to cut a release
candidate. Five workflows take `tags: ['v*.*.*']`, that pattern matches a
pre-release tag as happily as a final one, and every publish condition in them
read `startsWith(github.ref, 'refs/tags/v')`. So the tag armed the npm publish
step in wasm.yml, and `@pdfluent/sdk-wasm@1.0.0` went to the public registry
under the `latest` dist-tag before anyone could read the run. A release
candidate is by definition not the thing you publish; the tag that names it
published one anyway.

THE RULE. A tag whose name contains `-` is a pre-release, and a run whose ref
is such a tag publishes nothing -- not to npm, not to crates.io, not to a
container registry, and not as a GitHub Release. The rule is about the REF, not
about how the run started: a `workflow_dispatch` aimed at `v1.0.0-rc.3` is the
same publish with a person's name on it, and precondition 6 of #358 says a real
publish runs from the runbook on our own machines anyway.

The clause every publishing site carries:

    !(github.ref_type == 'tag' && contains(github.ref_name, '-'))

AND A PUBLISH NEEDS A TAG, NOT JUST AN INPUT. The npm jobs arm on a
dispatch decision (`inputs.publish_npm == 'true'`), and without a tag
requirement a branch dispatch with that input publishes from a branch.
Every such condition also carries `github.ref_type == 'tag'` (the same
clause release-binaries.yml's create-release already had), and this
guard evaluates each input-armed publishing site under a branch ref
with the publish inputs true: it must come out FALSE.

WHY THIS GUARD EVALUATES RATHER THAN GREPS. A grep for that line proves the
text is present, not that the condition is unreachable -- `A || B` with the
clause in `A` publishes on `B` regardless. So this reads the effective
condition of each publishing step (its own `if:`, ANDed with its job's `if:`,
ANDed with the conditions of the jobs it `needs:`) and evaluates it under a
pre-release tag with three-valued logic: anything it cannot know is MAYBE, and
MAYBE is not a pass. The condition must come out FALSE.

AND THE FLOOR, because a scan that finds nothing approves everything. Two of
them: at least PUBLISH_SITE_FLOOR publishing sites must be found at all, and in
every workflow that has one, at least one site must still be armed by a FINAL
tag. Without the second, deleting every publish step would make this guard
green, which is the other way to have no release.
"""
from __future__ import annotations
import argparse, pathlib, re, sys

import yaml

REPO = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_WORKFLOWS = REPO / ".github" / "workflows"

# The two workflows a `v*.*.*` tag starts. Held as a set so that a third one
# cannot be added without someone reading this file: a new tag-triggered
# workflow is exactly where the next accidental publish would live.
#
# node-bindings.yml, wasm.yml and release-binaries.yml left this set when
# their publishes went manual and gated: tags start nothing there now, and
# their publish jobs need a dispatch decision plus the release gate. Their
# publish steps are still sites below — the ref rule holds however the run
# started — but no final tag arms them.
TAG_TRIGGERED = {"docker.yml", "release-smoke.yml"}

# Four sites today: the npm publish in wasm.yml, publish.sh in
# node-bindings.yml (dispatch-only, still held to the ref rule), the GitHub
# Release in release-binaries.yml and the registry login in docker.yml.
# release-smoke.yml has none and is meant to have none. Raise this when a
# channel is added; never lower it without saying which channel went away.
PUBLISH_SITE_FLOOR = 4

# What counts as publishing: it leaves this repository and a stranger can fetch
# it. Substrings, matched against a step's `run:` text and its `uses:`.
PUBLISH_SIGNATURES = (
    "npm publish", "napi publish", "scripts/release/publish.sh",
    "cargo publish", "twine upload", "dotnet nuget push", "mvn deploy",
    "gh release create", "docker push",
    # `peter-evans/repository-dispatch` is deliberately absent. It asks another
    # repository to rebuild; nothing of ours reaches a registry through it, and
    # putting it here drags ci-ephemeral -- which no tag can start -- into a
    # rule about tags.
    "softprops/action-gh-release", "actions/create-release",
    # The credential is the hazard, not only the push: a step that logs in to a
    # registry on a pre-release tag has handed write access to the run. This is
    # docker.yml's only live site -- its push step is commented out -- and
    # gating the login means uncommenting it cannot publish an rc by surprise.
    "docker/login-action",
)

# `docker/build-push-action` is a publishing site only when it actually pushes.
# docker.yml builds with `push: false` on every run and that is not publishing;
# flipping it to true makes it one.
BUILD_PUSH_ACTION = "docker/build-push-action"


class Unknown:
    """A value this guard cannot know, e.g. `secrets.NPM_TOKEN` or `needs.x`."""
    __slots__ = ()
    def __repr__(self) -> str:  # pragma: no cover - debugging aid
        return "UNKNOWN"


UNKNOWN = Unknown()

# Three-valued truth. MAYBE is deliberately not a pass: a publish this guard
# cannot prove unreachable is a publish.
TRUE, FALSE, MAYBE = True, False, None


def truth(value) -> bool | None:
    """GitHub truthiness of an evaluated value, or MAYBE when unknowable."""
    if isinstance(value, Unknown):
        return MAYBE
    if isinstance(value, bool):
        return value
    if value is None:
        return False
    if isinstance(value, str):
        return value != ""
    if isinstance(value, (int, float)):
        return value != 0
    return True


# --------------------------------------------------------------------------
# A small evaluator for the subset of GitHub expressions that `if:` uses.
# --------------------------------------------------------------------------
TOKEN = re.compile(r"""
      \s+
    | (?P<string>'(?:[^']|'')*')
    | (?P<number>\d+(?:\.\d+)?)
    | (?P<op>&&|\|\||==|!=|>=|<=|>|<|!|\(|\)|,)
    | (?P<name>[A-Za-z_][A-Za-z_0-9.\-*]*)
""", re.VERBOSE)


class ParseError(Exception):
    pass


def tokenize(text: str) -> list[tuple[str, str]]:
    out: list[tuple[str, str]] = []
    i = 0
    while i < len(text):
        m = TOKEN.match(text, i)
        if not m or m.end() == i:
            raise ParseError(f"cannot read {text[i:i + 20]!r}")
        i = m.end()
        for kind in ("string", "number", "op", "name"):
            if m.group(kind) is not None:
                out.append((kind, m.group(kind)))
                break
    return out


class Parser:
    def __init__(self, tokens: list[tuple[str, str]], env: dict) -> None:
        self.t, self.i, self.env = tokens, 0, env

    def peek(self) -> tuple[str, str] | None:
        return self.t[self.i] if self.i < len(self.t) else None

    def take(self) -> tuple[str, str]:
        if self.i >= len(self.t):
            raise ParseError("expression ends early")
        self.i += 1
        return self.t[self.i - 1]

    def expect(self, value: str) -> None:
        kind, got = self.take()
        if got != value:
            raise ParseError(f"expected {value!r}, got {got!r}")

    # or := and ('||' and)*
    def parse_or(self):
        left = self.parse_and()
        while self.peek() and self.peek()[1] == "||":
            self.take()
            right = self.parse_and()
            left = or_(left, right)
        return left

    # and := cmp ('&&' cmp)*
    def parse_and(self):
        left = self.parse_cmp()
        while self.peek() and self.peek()[1] == "&&":
            self.take()
            right = self.parse_cmp()
            left = and_(left, right)
        return left

    # cmp := unary (('=='|'!='|'<'|'<='|'>'|'>=') unary)?
    def parse_cmp(self):
        left = self.parse_unary()
        nxt = self.peek()
        if nxt and nxt[1] in ("==", "!=", "<", "<=", ">", ">="):
            op = self.take()[1]
            right = self.parse_unary()
            return compare(op, left, right)
        return left

    def parse_unary(self):
        nxt = self.peek()
        if nxt and nxt[1] == "!":
            self.take()
            return not_(self.parse_unary())
        return self.parse_primary()

    def parse_primary(self):
        kind, value = self.take()
        if value == "(":
            inner = self.parse_or()
            self.expect(")")
            return inner
        if kind == "string":
            return value[1:-1].replace("''", "'")
        if kind == "number":
            return float(value)
        if kind != "name":
            raise ParseError(f"unexpected {value!r}")
        if self.peek() and self.peek()[1] == "(":
            self.take()
            args = []
            if self.peek() and self.peek()[1] != ")":
                args.append(self.parse_or())
                while self.peek() and self.peek()[1] == ",":
                    self.take()
                    args.append(self.parse_or())
            self.expect(")")
            return call(value, args)
        return lookup(value, self.env)


def lookup(name: str, env: dict):
    if name == "true":
        return True
    if name == "false":
        return False
    if name == "null":
        return None
    if name in env:
        return env[name]
    # `inputs.*` is a single decision: either the whole context is known (a
    # push, where every input is empty) or none of it is.
    root = name.split(".", 1)[0]
    if root in env:
        return env[root]
    return UNKNOWN


def call(name: str, args: list):
    lowered = name.lower()
    if lowered == "always":
        return True
    if lowered in ("cancelled", "failure", "success"):
        return UNKNOWN
    if lowered in ("startswith", "endswith", "contains"):
        if any(isinstance(a, Unknown) for a in args) or len(args) != 2:
            return UNKNOWN
        # CASE-INSENSITIVELY, the way GitHub does it. Python's are not, and the
        # difference is not academic: `contains(github.ref_name, '-RC.')` is
        # false here and true on GitHub for `v1.0.0-rc.2`, so a condition that
        # publishes a release candidate would have read as safe. (codex, #1906)
        a, b = (str(x).lower() for x in args)
        if lowered == "startswith":
            return a.startswith(b)
        if lowered == "endswith":
            return a.endswith(b)
        return b in a
    if lowered == "format":
        if any(isinstance(a, Unknown) for a in args) or not args:
            return UNKNOWN
        out = str(args[0])
        for n, a in enumerate(args[1:]):
            out = out.replace("{%d}" % n, str(a))
        return out
    return UNKNOWN


def compare(op: str, a, b):
    if isinstance(a, Unknown) or isinstance(b, Unknown):
        return UNKNOWN
    # GitHub compares loosely; the only coercion that reaches us is null/'' .
    a = "" if a is None else a
    b = "" if b is None else b
    if isinstance(a, bool) != isinstance(b, bool):
        a, b = str(a).lower() if isinstance(a, bool) else a, str(b).lower() if isinstance(b, bool) else b
    # Same rule for `==` and `!=`: GitHub's string comparison ignores case.
    if isinstance(a, str) and isinstance(b, str):
        a, b = a.lower(), b.lower()
    try:
        if op == "==":
            return a == b
        if op == "!=":
            return a != b
        if op == "<":
            return a < b
        if op == "<=":
            return a <= b
        if op == ">":
            return a > b
        return a >= b
    except TypeError:
        return UNKNOWN


def not_(value):
    t = truth(value)
    return UNKNOWN if t is MAYBE else (not t)


def and_(a, b):
    ta, tb = truth(a), truth(b)
    if ta is False or tb is False:
        return False
    if ta is True and tb is True:
        return True
    return UNKNOWN


def or_(a, b):
    ta, tb = truth(a), truth(b)
    if ta is True or tb is True:
        return True
    if ta is False and tb is False:
        return False
    return UNKNOWN


def evaluate(expression: str, env: dict) -> bool | None:
    """Three-valued truth of a GitHub `if:` expression under `env`."""
    text = expression.strip()
    # `if:` may be written bare or wrapped once in ${{ }}; both are the same
    # expression. Anything else with a ${{ }} inside it is string building, and
    # this guard will not guess what it builds.
    if text.startswith("${{") and text.endswith("}}") and text.count("${{") == 1:
        text = text[3:-2].strip()
    elif "${{" in text:
        return MAYBE
    if not text:
        return True
    return truth(Parser(tokenize(text), env).parse_or())


# --------------------------------------------------------------------------
# Reading the workflows
# --------------------------------------------------------------------------
def tag_env(ref_name: str, *, dispatchable: bool) -> dict:
    """The context of a run whose ref is the tag `ref_name`.

    `dispatchable` false means "a push", where every `workflow_dispatch` input
    is empty. True leaves the inputs UNKNOWN, which is how the negative case is
    evaluated: the rule is about the ref, so it must hold however the run
    started.
    """
    return {
        "github.ref": f"refs/tags/{ref_name}",
        "github.ref_name": ref_name,
        "github.ref_type": "tag",
        "github.event_name": UNKNOWN if dispatchable else "push",
        "inputs": UNKNOWN if dispatchable else "",
        "github.event.inputs": UNKNOWN if dispatchable else "",
    }


# The pre-release tags every publishing site must be FALSE for. These four are
# the floor, not the whole list -- see `candidate_tags`.
PRE_RELEASE_TAGS = ("v1.0.0-rc.2", "v1.0.0-beta.19", "v2.0.0-alpha.1", "v1.0.0-0")
FINAL_TAG = "v1.0.0"

# The dispatch decisions that arm a publish. A publishing site whose
# condition names one of these is evaluated under a branch ref with the
# decision true -- and must come out FALSE. Anything else (a push-gated
# login, a tag-gated release) is not input-armed and keeps only the
# pre-release rule.
PUBLISH_INPUTS = ("inputs.publish_npm", "inputs.create_release")


def branch_env() -> dict:
    """A dispatch on a branch with every publish input true."""
    return {
        "github.ref": "refs/heads/t6/packaging-every-platform",
        "github.ref_name": "t6/packaging-every-platform",
        "github.ref_type": "branch",
        "github.event_name": "workflow_dispatch",
        "inputs.publish_npm": "true",
        "inputs.create_release": "true",
    }

LITERAL = re.compile(r"'((?:[^']|'')*)'")


def candidate_tags(conditions: list[str]) -> list[str]:
    """Pre-release tags built out of the condition's own string literals.

    FOUR FIXED TAGS ARE A SAMPLE, NOT THE RULE. `!contains(github.ref_name,
    '-') || startsWith(github.ref_name, 'v3.')` is false for all four and true
    for the final tag, so a fixed list reports it safe while `v3.0.0-rc.1` arms
    the publisher. (codex, #1906)

    A condition made of `startsWith`/`endsWith`/`contains`/`==` can only tell
    two ref names apart by the literals written in it, so the literals are
    where the adversarial names come from: each one is tried as a tag, as a
    prefix, and as a suffix. `'v3.'` becomes `v3.-rc.1`, which starts with
    `v3.` and is a pre-release; `'-final'` becomes `v1.0.0--final`. Only
    candidates that are themselves pre-releases (they contain `-`) are used --
    the rule says nothing about the others.
    """
    names = list(PRE_RELEASE_TAGS)
    for condition in conditions:
        for raw in LITERAL.findall(condition):
            literal = raw.replace("''", "'")
            if literal.startswith("refs/tags/"):
                literal = literal[len("refs/tags/"):]
            if not literal or len(literal) > 64 or "\n" in literal:
                continue
            for candidate in (literal, f"{literal}-rc.1", f"v1.0.0-{literal}",
                              f"v1.0.0-{literal}-rc.1"):
                if "-" in candidate and candidate not in names:
                    names.append(candidate)
    return names


def publish_signature(step: dict) -> str | None:
    """The reason this step publishes, or None."""
    uses = str(step.get("uses") or "")
    run = str(step.get("run") or "")
    haystack = f"{uses}\n{run}"
    for needle in PUBLISH_SIGNATURES:
        if needle in haystack:
            return needle
    if BUILD_PUSH_ACTION in uses:
        push = (step.get("with") or {}).get("push")
        if push is not False and str(push).strip().lower() not in ("false", "none", ""):
            return f"{BUILD_PUSH_ACTION} (push: {push})"
    return None


# `always()`, `cancelled()` and `failure()` in a job's own `if:` replace the
# `success()` GitHub otherwise prefixes, and `success()` is the whole of what
# makes a skipped dependency skip the job that waits for it. A job carrying one
# of them runs even when everything it needs was skipped.
STATUS_OVERRIDE = re.compile(r"\b(always|cancelled|failure)\s*\(")


def job_condition(jobs: dict, name: str, seen: frozenset[str] = frozenset()) -> list[str]:
    """A job's own `if:` plus the `if:` of every job it waits for.

    A job whose dependency is skipped is skipped, so a gate on `build-release`
    also gates the `publish-npm` that needs it. Modelled rather than assumed,
    because it is the difference between one clause and four.

    UNLESS the job overrides the status check. `needs: build` with
    `if: always()` runs even when `build` was skipped, so inheriting `build`'s
    gate there would report a publish as safely FALSE while it publishes the
    release candidate -- the clause would be on the one job that does not run.
    (codex, #1906)
    """
    if name in seen:
        return []
    job = jobs.get(name)
    if not isinstance(job, dict):
        return []
    out: list[str] = []
    own = None if job.get("if") is None else str(job["if"])
    if own is not None:
        out.append(own)
        if STATUS_OVERRIDE.search(own):
            return out
    needs = job.get("needs")
    if isinstance(needs, str):
        needs = [needs]
    for dep in (needs or []):
        out.extend(job_condition(jobs, str(dep), seen | {name}))
    return out


def and3(a: bool | None, b: bool | None) -> bool | None:
    """AND over three-valued truths.

    Not `and_`: that one works on evaluated VALUES, where `None` is GitHub's
    null and falsey. Here `None` is MAYBE, and MAYBE and TRUE is MAYBE. Reusing
    `and_` here silently turned every MAYBE into FALSE -- which cleared exactly
    the publishing steps this guard cannot vouch for. Found by the test.
    """
    if a is False or b is False:
        return False
    if a is True and b is True:
        return True
    return None


def effective(conditions: list[str], env: dict) -> bool | None:
    result: bool | None = True
    for c in conditions:
        result = and3(result, evaluate(c, env))
    return result


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--workflows", type=pathlib.Path, default=DEFAULT_WORKFLOWS,
                    help="directory of workflow files to read")
    ap.add_argument("--floor", type=int, default=PUBLISH_SITE_FLOOR,
                    help="fewest publishing sites this scan may find")
    ap.add_argument("--expect-tag-triggered", default=",".join(sorted(TAG_TRIGGERED)),
                    help="comma-separated workflow files a version tag may start")
    args = ap.parse_args(argv)

    if not args.workflows.is_dir():
        print(f"[prerelease-publish] FATAL: {args.workflows} is not a directory",
              file=sys.stderr)
        return 2
    expected_triggered = {n for n in args.expect_tag_triggered.split(",") if n}

    problems: list[str] = []
    sites = 0
    armed_by_final: dict[str, int] = {}
    has_site: set[str] = set()
    triggered: set[str] = set()

    paths = sorted(args.workflows.glob("*.yml")) + sorted(args.workflows.glob("*.yaml"))
    for path in paths:
        try:
            doc = yaml.safe_load(path.read_text()) or {}
        except yaml.YAMLError as e:
            print(f"[prerelease-publish] FATAL: {path.name} will not parse: {e}",
                  file=sys.stderr)
            return 2
        if not isinstance(doc, dict):
            continue
        # `on` is YAML 1.1 true; both spellings appear depending on the loader.
        on = doc.get("on", doc.get(True)) or {}
        if isinstance(on, dict):
            push = on.get("push") or {}
            if isinstance(push, dict) and push.get("tags"):
                triggered.add(path.name)

        jobs = doc.get("jobs") or {}
        if not isinstance(jobs, dict):
            continue
        for job_name, job in jobs.items():
            if not isinstance(job, dict):
                continue
            outer = job_condition(jobs, str(job_name))
            for index, step in enumerate(job.get("steps") or []):
                if not isinstance(step, dict):
                    continue
                why = publish_signature(step)
                if why is None:
                    continue
                sites += 1
                has_site.add(path.name)
                label = (f"{path.name}:{job_name}:"
                         f"{step.get('name') or step.get('uses') or f'step {index}'}")
                conditions = list(outer)
                if step.get("if") is not None:
                    conditions.append(str(step["if"]))
                try:
                    for tag in candidate_tags(conditions):
                        env = tag_env(tag, dispatchable=True)
                        verdict = effective(conditions, env)
                        if verdict is not False:
                            problems.append(
                                f"{label} publishes ({why}) and its condition is "
                                f"{'MAYBE' if verdict is MAYBE else 'TRUE'} on the "
                                f"pre-release tag {tag}. Effective condition: "
                                f"{' && '.join(conditions) or '(none)'}")
                            break
                    if any(d in c for d in PUBLISH_INPUTS for c in conditions):
                        branch = effective(conditions, branch_env())
                        if branch is not False:
                            problems.append(
                                f"{label} publishes ({why}) from a branch dispatch "
                                f"with the publish input true. A publish needs a tag, "
                                f"not just an input: the condition must also carry "
                                f"`github.ref_type == 'tag'`. Effective condition: "
                                f"{' && '.join(conditions) or '(none)'}")
                    final = effective(conditions, tag_env(FINAL_TAG, dispatchable=False))
                except ParseError as e:
                    print(f"[prerelease-publish] FATAL: {label} has a condition "
                          f"this guard cannot read ({e}). A condition nobody can "
                          "read is not one that can be cleared.", file=sys.stderr)
                    return 2
                if final is not False:
                    armed_by_final[path.name] = armed_by_final.get(path.name, 0) + 1

    missing = expected_triggered - triggered
    extra = triggered - expected_triggered
    if missing:
        problems.append(
            f"expected a version tag to start {sorted(missing)}, and no push-tag "
            "trigger was found there. Either the trigger went away or the file "
            "was renamed; the list in this guard is out of date either way.")
    if extra:
        problems.append(
            f"{sorted(extra)} is started by a tag and this guard did not know "
            "about it. Read its publish steps, then add it to TAG_TRIGGERED.")

    # Only for the workflows a tag starts. A workflow that publishes on
    # dispatch alone (there are none today) has no final tag to be armed by.
    for name in sorted(has_site & triggered):
        if not armed_by_final.get(name):
            problems.append(
                f"{name} has a publishing step and NONE of them is armed by the "
                f"final tag {FINAL_TAG}. A release that publishes nothing passes "
                "the pre-release rule by not being a release.")

    if sites < args.floor:
        print(f"[prerelease-publish] FATAL: found {sites} publishing step(s), "
              f"fewer than the floor of {args.floor}. A scan that finds nothing "
              "approves everything -- either a channel was removed (say which, "
              "and lower the floor) or the signatures no longer match it.",
              file=sys.stderr)
        return 2

    if problems:
        print(f"[prerelease-publish] FAIL: {len(problems)} problem(s):", file=sys.stderr)
        for p in problems:
            print(f"    {p}", file=sys.stderr)
        if any("branch dispatch" in p for p in problems):
            print("\n  An npm publish needs a tag, not just an input: a branch\n"
                  "  dispatch with the publish input true must arm nothing. Add\n"
                  "  to the step or its job:\n\n"
                  "      && github.ref_type == 'tag'\n", file=sys.stderr)
        if any("branch dispatch" not in p for p in problems):
            print("\n  A pre-release tag (one whose name contains '-') must arm no\n"
                  "  publish job. Add to the step or its job:\n\n"
                  "      if: >-\n"
                  "        !(github.ref_type == 'tag' && contains(github.ref_name, '-'))\n"
                  "        && <the condition it already had>\n", file=sys.stderr)
        return 1

    print(f"[prerelease-publish] OK: {sites} publishing step(s) in "
          f"{len(has_site)} workflow(s); none reachable on a pre-release tag, "
          f"and each of those workflows still publishes on {FINAL_TAG}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
