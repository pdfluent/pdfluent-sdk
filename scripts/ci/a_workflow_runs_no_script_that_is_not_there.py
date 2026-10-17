#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Every script a GitHub workflow runs is in the tree (#358).

`ci_config_lint.py` has checked this for `.gitlab-ci.yml` since a release job
was found calling a `publish_ordered.sh` that had never been committed. Nothing
checked the GitHub workflows, and the same shape was sitting there:
`publish-crates.yml` called `./scripts/publish_ordered.sh` at three steps while
the script lives at `scripts/release/publish_ordered.sh`. The only hosted
crates.io publish path would have died in seconds, on the one day it mattered.

WHAT IT READS. Only `run:` bodies -- what the runner executes -- and only
literal repository-relative paths in them: a token starting `scripts/` or
`./scripts/` and ending `.sh` or `.py`, with no shell expansion, wildcard or
variable in it. A path assembled at runtime is not something a file check can
judge, and pretending otherwise would make this guard noisy enough to turn off.

WHERE IT LOOKS. Against the step's working directory and nothing else.
`working-directory` on the step, on the job's `defaults.run`, or on the
workflow's, in that order -- `crates/pdf-node` runs `scripts/release/publish.sh`
and means its own, which a root-relative check calls missing and is wrong
about. There is no fallback to the repository root: the runner does not have
one either, so a root copy standing in for the one the step needs would make
this guard green about a workflow that fails.

Known-missing paths live in workflow_scripts_not_in_the_tree.toml with a reason
and a ticket. The guard fails when one of those has since appeared, so the file
cannot quietly outlive the debt it records.

Exit 0 when every such path exists, 1 otherwise, naming workflow, job and path.
"""
from __future__ import annotations

import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = REPO / ".github" / "workflows"
KNOWN = pathlib.Path(__file__).with_name("workflow_scripts_not_in_the_tree.toml")

# A literal path to a script in this repository. Anything carrying $ { } * ? or
# a backtick is assembled rather than written and is left alone.
SCRIPT = re.compile(r"(?<![\w./-])(\./)?(scripts/[A-Za-z0-9_./-]+\.(?:sh|py))")


def without_comment(line: str) -> str:
    """The part of a shell line the runner executes.

    A `#` starts a comment only outside quotes and at the start of a word --
    `echo "a # b"` and `curl host/#frag` are not comments. Dropping only
    whole-line comments left `echo done  # scripts/release/removed.sh` looking
    like a missing executable, and a guard that fails on the sentence
    explaining why a path is gone teaches people to delete the sentence.
    (codex, #1915)
    """
    quote = ""
    escaped = False
    for index, char in enumerate(line):
        if escaped:
            escaped = False
            continue
        # A backslash escapes inside double quotes and outside quotes, but not
        # inside single quotes, where bash takes every byte literally. Treating
        # `echo "a \\" # still quoted"` as closing at the escaped quote
        # truncated the line and hid the command after it. (codex, #1915)
        if char == "\\" and quote != "'":
            escaped = True
            continue
        if quote:
            if char == quote:
                quote = ""
            continue
        if char in "'\"":
            quote = char
        elif char == "#" and (index == 0 or line[index - 1] in " \t;&|("):
            return line[:index]
    return line


def named_scripts(body: str) -> set[str]:
    found = set()
    for raw in body.splitlines():
        line = without_comment(raw)
        for match in SCRIPT.finditer(line):
            path = match.group(2)
            if any(ch in path for ch in "${}*?`"):
                continue
            found.add(path)
    return found


def _default_dir(node) -> str | None:
    """`defaults: run: working-directory:` of a workflow or a job."""
    if isinstance(node, dict):
        run = node.get("defaults", {})
        if isinstance(run, dict):
            inner = run.get("run", {})
            if isinstance(inner, dict):
                where = inner.get("working-directory")
                if isinstance(where, str):
                    return where
    return None


def run_bodies(node, trail: str = "", where: str | None = None):
    """Every `run:` body, with the path that reaches it and its directory."""
    out: list[tuple[str, str, str]] = []
    if isinstance(node, dict):
        here = _default_dir(node) or where
        if "run" in node and isinstance(node["run"], str):
            step_dir = node.get("working-directory")
            out.append((trail, node["run"],
                        step_dir if isinstance(step_dir, str) else (here or "")))
        for key, value in node.items():
            if key == "run":
                continue
            out += run_bodies(value, f"{trail}.{key}" if trail else str(key), here)
    elif isinstance(node, list):
        for index, value in enumerate(node):
            out += run_bodies(value, f"{trail}[{index}]", where)
    return out


def known_missing() -> dict[tuple[str, str, str], str]:
    """The registered exceptions, keyed by (workflow, working directory, path).

    The directory is part of the key because the same written path under two
    working directories is two different files. Keyed on the pair alone, the
    secret-gated `scripts/generate_reference_pngs.py` exception would also
    exempt a new, unconditional step naming that path beneath a
    `working-directory`. (codex, #1915)
    """
    if not KNOWN.is_file():
        return {}
    import tomllib  # 3.11+, which is what every other TOML reader here assumes
    data = tomllib.loads(KNOWN.read_text(encoding="utf-8"))
    out = {}
    for entry in data.get("missing", []):
        # THE REASON AND THE TICKET ARE THE EXCEPTION. Without them an entry is
        # a workflow/path pair that exempts a missing executable and says
        # nothing about why or when it goes -- which is the hiding this
        # register exists to prevent. (codex, #1915)
        try:
            workflow, path = entry["workflow"], entry["path"]
        except KeyError as why:
            raise SystemExit(f"{KNOWN.name}: an entry is missing {why}")
        reden = entry.get("reden", "")
        if not isinstance(reden, str) or not reden.strip():
            raise SystemExit(f"{KNOWN.name}: {workflow} / {path} has no reden")
        # `type(...) is int`, not isinstance: bool is a subclass of int in
        # Python, so `ticket = true` would pass an isinstance check and exempt
        # a missing script with no removal ticket. (codex, #1915)
        ticket = entry.get("ticket")
        if type(ticket) is not int or ticket <= 0:
            raise SystemExit(f"{KNOWN.name}: {workflow} / {path} has no ticket")
        directory = entry.get("working_directory", "")
        if not isinstance(directory, str):
            raise SystemExit(f"{KNOWN.name}: {workflow} / {path} has a "
                             f"working_directory that is not a string")
        out[(workflow, directory, path)] = reden
    return out


def main() -> int:
    try:
        import yaml
    except ImportError:
        print("SKIPPED (not a pass): pyyaml is missing, so no workflow was read",
              file=sys.stderr)
        return 1

    if not WORKFLOWS.is_dir():
        print(f"SKIPPED (not a pass): no {WORKFLOWS}", file=sys.stderr)
        return 1

    files = sorted(list(WORKFLOWS.glob("*.yml")) + list(WORKFLOWS.glob("*.yaml")))
    if not files:
        print("SKIPPED (not a pass): no workflow files to read", file=sys.stderr)
        return 1

    registered = known_missing()
    missing: list[str] = []
    stale: list[str] = []
    seen_registered: set[tuple[str, str]] = set()
    checked = 0
    for workflow in files:
        try:
            document = yaml.safe_load(workflow.read_text(encoding="utf-8"))
        except yaml.YAMLError as why:
            print(f"{workflow.name}: not readable as YAML: {why}", file=sys.stderr)
            return 1
        for trail, body, directory in run_bodies(document):
            for path in named_scripts(body):
                checked += 1
                # The runner resolves a relative command beneath
                # `working-directory` and NOWHERE else, so when one is set that
                # is the only place to look. Falling back to the repository
                # root makes the guard green on a workflow that will fail --
                # the root copy exists, the one the step actually needs does
                # not. (codex, #1915)
                where = REPO / directory / path if directory else REPO / path
                if where.is_file():
                    continue
                key = (workflow.name, directory, path)
                if key in registered:
                    seen_registered.add(key)
                    print(f"SKIPPED (not a pass): {workflow.name} runs {path}, "
                          f"which is not in the tree — registered in "
                          f"{KNOWN.name}", file=sys.stderr)
                    continue
                # The directory is in the message because two steps can name
                # the same path and mean different files.
                beneath = f" beneath {directory}" if directory else ""
                missing.append(f"{workflow.name} ({trail}) runs {path}"
                               f"{beneath}, which is not in the tree")

    for key in registered:
        if key not in seen_registered:
            named = f"{key[0]} / {key[2]}" + (f" (in {key[1]})" if key[1] else "")
            stale.append(f"{named} is registered as missing but the workflow "
                         f"no longer names it, or the file is back")
    if stale:
        print(f"[workflow-scripts] {len(stale)} stale exception(s) in "
              f"{KNOWN.name}:", file=sys.stderr)
        for line in stale:
            print(f"  {line}", file=sys.stderr)
        return 1

    if missing:
        print(f"[workflow-scripts] {len(missing)} missing:", file=sys.stderr)
        for line in missing:
            print(f"  {line}", file=sys.stderr)
        return 1
    print(f"[workflow-scripts] OK: {checked} script path(s) named in "
          f"{len(files)} workflow(s), all present")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
