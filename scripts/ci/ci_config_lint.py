#!/usr/bin/env python3
"""Check .gitlab-ci.yml for faults that valid YAML happily hides.

WHY THIS EXISTS

`yaml.safe_load` returning without an exception says the file is YAML. It says
nothing about whether GitLab can run it. On 2026-08-19 this line

    - echo "[binding-python] own target dir: ${CARGO_TARGET_DIR}"

parsed cleanly and produced a *dictionary* inside the script list, because a plain
YAML scalar containing ": " is a mapping. GitLab rejected the whole file, the
pipeline came back with status `failed` and ZERO jobs, and nothing in the job list
pointed at the cause. My local check had said "yaml ok".

So this checks the shape, not just the syntax. Every gate we have is worthless if
the pipeline that runs them will not start.

Exit codes:
    0  fine
    1  a fault that would break the pipeline
    2  could not run
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
CI = REPO / ".gitlab-ci.yml"


def flatten(item) -> list:
    """GitLab allows nested arrays of strings; flatten to check the leaves."""
    if isinstance(item, list):
        out = []
        for sub in item:
            out.extend(flatten(sub))
        return out
    return [item]



# Vlaggen die aan een Python-script in dit repo worden meegegeven maar die het
# script niet kent.
#
# corpus:text-replace-capability gaf `--fallback standard` mee aan een gate die
# die vlag niet had. Argparse weigerde, de job viel binnen 0,4 seconde om, en het
# cijfer dat hij moest opleveren is nooit gemeten -- maandenlang, want een job die
# meteen faalt op een handmatige trigger valt niemand op. De vlag stond in de
# config, het script kende hem niet, en niets vergeleek die twee.
#
# Dit vergelijkt ze. Per commando dat begint met `python3 <pad in de repo>` worden
# de meegegeven lange vlaggen opgezocht in de `--help` van dat script.
COMMANDO_SCHEIDERS = re.compile(r"\s*(?:\|\||&&|\||;|\n)\s*")


def python_commandos(blok: str):
    """Geeft (scriptpad, [vlaggen]) voor elk python3-commando in een scriptblok."""
    # Regelvervolgen samenvoegen: een commando dat met \ eindigt loopt door.
    samen = re.sub(r"\\\s*\n\s*", " ", blok)
    for stuk in COMMANDO_SCHEIDERS.split(samen):
        stuk = stuk.strip()
        m = re.match(r"python3?\s+(\S+\.py)\b(.*)", stuk, re.S)
        if not m:
            continue
        pad = REPO / m.group(1)
        if not pad.is_file():
            continue
        vlaggen = set(re.findall(r"(?<![\w-])(--[a-z][a-z0-9-]*)", m.group(2)))
        yield pad, vlaggen


def onbekende_vlaggen(pad: Path, vlaggen: set) -> list:
    if not vlaggen:
        return []
    # Alleen fouten opvangen die écht over het script gaan. Een brede `except`
    # maakte deze controle stil kapot: `subprocess` was niet geïmporteerd, de
    # NameError verdween erin, en de lint meldde vrolijk dat alles in orde was
    # terwijl hij niets deed. Dat is dezelfde fout als degene die hij moet vangen.
    try:
        uit = subprocess.run(
            [sys.executable, str(pad), "--help"],
            capture_output=True, text=True, timeout=60,
        )
    except (OSError, subprocess.SubprocessError):
        return []
    hulptekst = uit.stdout + uit.stderr
    if "usage" not in hulptekst.lower():
        return []  # geen argparse; niets te vergelijken
    bekend = set(re.findall(r"(--[a-z][a-z0-9-]*)", hulptekst))
    return sorted(v for v in vlaggen if v not in bekend)


def main() -> None:
    try:
        import yaml
    except ImportError:
        print("[ci_config_lint] FATAL: pyyaml missing", file=sys.stderr)
        sys.exit(2)

    try:
        doc = yaml.safe_load(CI.read_text())
    except Exception as e:  # noqa: BLE001
        print(f"[ci_config_lint] FAIL: not valid YAML at all: {e}")
        sys.exit(1)

    problems: list[str] = []
    job_names = {n for n, j in doc.items() if not n.startswith(".") and isinstance(j, dict)}

    for name, job in doc.items():
        if not isinstance(job, dict):
            continue
        for key in ("script", "before_script", "after_script"):
            block = job.get(key)
            if block is None:
                continue
            if isinstance(block, str):
                continue
            if not isinstance(block, list):
                problems.append(f"{name}.{key} is a {type(block).__name__}, expected a list")
                continue
            for leaf in flatten(block):
                if isinstance(leaf, str):
                    continue
                # This is the ": " trap. Name it precisely, because the symptom
                # (a pipeline with no jobs) points nowhere near the cause.
                hint = ""
                if isinstance(leaf, dict):
                    k = next(iter(leaf), "")
                    hint = (f'  -- looks like an unquoted ": " in: {k!r}. '
                            "Wrap the whole command in single quotes.")
                problems.append(
                    f"{name}.{key} contains a {type(leaf).__name__}, not a string.{hint}"
                )

        # Hardcoded target/ paths. CARGO_TARGET_DIR is set for every job on this
        # runner, so `target/release/x` is a directory cargo never writes to. The
        # failure is expensive and misleading: the job builds successfully for
        # minutes, then reports the binary as missing. This has now happened three
        # times in one day -- the C ABI Makefile, and both PDF/A corpus gates,
        # which each compiled for six minutes before looking in the wrong place.
        for key in ("script", "before_script", "after_script"):
            for leaf in flatten(job.get(key) or []):
                if not isinstance(leaf, str):
                    continue
                for line in leaf.splitlines():
                    if re.search(r"(?<![\w/${])target/(release|debug)/", line) \
                            and "CARGO_TARGET_DIR" not in line:
                        problems.append(
                            f"{name}.{key} hardcodes a target/ path: {line.strip()[:70]!r}"
                            '  -- use "${CARGO_TARGET_DIR:-target}/release/..."'
                        )

        # A `needs` on a job that does not exist stops the pipeline from being
        # created at all, with the same unhelpful "failed, no jobs" symptom.
        for dep in job.get("needs") or []:
            dep_name = dep.get("job") if isinstance(dep, dict) else dep
            if isinstance(dep_name, str) and dep_name not in job_names:
                problems.append(f"{name}.needs refers to {dep_name!r}, which is not a job")

    for name, job in doc.items():
        if not isinstance(job, dict) or name.startswith("."):
            continue
        for key in ("script", "before_script", "after_script"):
            blok = job.get(key)
            if not isinstance(blok, list):
                continue
            for pad, vlaggen in python_commandos("\n".join(str(r) for r in blok)):
                for vlag in onbekende_vlaggen(pad, vlaggen):
                    problems.append(
                        f"{name}: passes {vlag} to {pad.relative_to(REPO)}, "
                        "which does not accept it — the job dies on argument parsing"
                    )

    if problems:
        print(f"[ci_config_lint] FAIL: {len(problems)} problem(s) GitLab would reject:")
        for p in problems:
            print(f"  - {p}")
        print()
        print("[ci_config_lint] These parse as valid YAML. GitLab still refuses them,")
        print("[ci_config_lint] and the failure arrives as a pipeline with no jobs at all.")
        sys.exit(1)

    print(f"[ci_config_lint] {len(job_names)} jobs, all script entries are strings, "
          "all needs resolve")
    sys.exit(0)


if __name__ == "__main__":
    main()
