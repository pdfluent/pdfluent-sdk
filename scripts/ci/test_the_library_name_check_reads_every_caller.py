#!/usr/bin/env python3
"""The Java library-name check must read every caller, not a list of three.

The step in java-bindings.yml exists because Cargo.toml built libpdfluent_java
while the Java side loaded pdf_java. It guarded that with three file paths typed
out by hand, while five files under crates/pdf-java called System.loadLibrary --
and two of the uncovered ones arrived in the very change that added the check.
A list kept by hand drifts away from the tree silently, which is the same shape
of failure the step was written to catch.

So this test does not paraphrase the check: it lifts the step's own `run` body
out of the workflow and executes it against trees it builds, including one tree
per caller with that caller's name changed. If any single call could be renamed
without the step noticing, one of those runs comes back green and this test
fails.

THE FLOOR IS READ, NOT REPEATED (#361)

This file held its own copy of the step's floor, and on 07-09-2026 the two said
different things: the fifth caller was deleted on purpose with the licence key
in f9ae3a1d7, the step still demanded five, and this test still agreed with the
step -- so both were red on master and in the wave, for a deletion that was the
point of the change. Two copies of one number drift apart; that is not a risk,
it is what happened. The floor now comes out of the shipped step body, so there
is one number and this test measures whether it matches the tree.
"""
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

import yaml

ROOT = pathlib.Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/java-bindings.yml"
CRATE = "crates/pdf-java"
LIB_NAME = "pdfluent_java"
# The floor the shipped step enforces, wherever it stands. A floor and not an
# equality: adding a caller is normal, and the check is meant to cover a new one
# rather than to forbid it.
FLOOR = re.compile(r'"\$\{#loaders\[@\]\}"\s+-lt\s+(\d+)')

failures: list[str] = []


def fail(line: str) -> None:
    failures.append(line)


def step_body() -> str | None:
    """The shipped text of the name-checking step, or None if it is gone.

    Recognised by its subject -- a run body that mentions System.loadLibrary --
    and not by any variable this fix happens to use. Keyed on `loaders`, this
    would report a hand-listed step as no step at all, and the check for a
    returning hand-list below would never run.
    """
    doc = yaml.safe_load(WORKFLOW.read_text())
    candidates = [
        stap["run"]
        for job in doc.get("jobs", {}).values()
        for stap in (job.get("steps", []) or [])
        if "System.loadLibrary" in (stap.get("run") or "")
    ]
    if len(candidates) > 1:
        fail(f"{len(candidates)} steps check the loaded library name; this test "
            "reads one of them and would miss a regression in the others")
    return candidates[0] if candidates else None


def build_tree(base: pathlib.Path, files: dict[str, str]) -> None:
    """A tree the step can run in: the sources, plus the .so it tests for."""
    built = base / "target/debug"
    built.mkdir(parents=True, exist_ok=True)
    (built / f"lib{LIB_NAME}.so").write_bytes(b"")
    for path, content in files.items():
        target = base / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content)


def run_step(lichaam: str, files: dict[str, str]) -> subprocess.CompletedProcess:
    with tempfile.TemporaryDirectory() as tmp:
        base = pathlib.Path(tmp)
        build_tree(base, files)
        return subprocess.run(
            ["bash", "-c", lichaam],
            cwd=base,
            capture_output=True,
            text=True,
        )


def real_sources() -> dict[str, str]:
    """Every .java file under the crate, as it stands in the repository."""
    return {
        str(p.relative_to(ROOT)): p.read_text()
        for p in sorted((ROOT / CRATE).rglob("*.java"))
    }


def main() -> int:
    if not WORKFLOW.exists():
        print(f"[java-name] FAIL: {WORKFLOW.relative_to(ROOT)} is gone")
        return 1

    lichaam = step_body()
    if lichaam is None:
        print("[java-name] FAIL: no step in java-bindings.yml checks the loaded "
              "library name; the name went out of step once already")
        return 1

    # 1. The list is derived, not typed. A `for f in .../Foo.java \` list is
    #    the thing being removed; catching its return is the point.
    if re.search(r"for f in\s+\S*/\S+\.java", lichaam):
        fail("the step enumerates .java paths by hand again; derive them with a "
            "glob over the crate instead")
    if f"{CRATE} --include" not in lichaam and f"{CRATE}' --include" not in lichaam:
        fail(f"the step does not glob over {CRATE}")

    bronnen = real_sources()
    callers = {p: t for p, t in bronnen.items() if "System.loadLibrary" in t}

    # 2. The floor the step enforces, taken from the step and not from a second
    #    copy here. A floor of nought would let an empty tree through, which is
    #    the case the line exists for.
    gevonden = FLOOR.search(lichaam)
    if gevonden is None:
        fail("the step no longer holds a floor on the number of callers; an empty "
            "glob then walks the loop zero times and passes without reading a file")
        vloer = 0
    else:
        vloer = int(gevonden.group(1))
        if vloer < 1:
            fail(f"the step's floor is {vloer}, which an empty tree satisfies")
        if len(callers) < vloer:
            fail(f"the step demands at least {vloer} System.loadLibrary callers "
                f"under {CRATE} and the tree holds {len(callers)}: {sorted(callers)}. "
                "Either a caller was lost -- restore it -- or the number is stale "
                "and the commit that lowers it says which caller went and where.")

    # 3. The tree as it stands passes. Without this the red runs below prove
    #    only that the step can fail, not that it can tell the cases apart.
    clean = run_step(lichaam, bronnen)
    if clean.returncode != 0:
        fail(f"the step fails on the repository as it stands (rc="
            f"{clean.returncode}): {clean.stderr.strip()[:400]}")

    # 4. The mutation, one caller at a time. Every one of them must be able to
    #    turn the step red on its own -- that is what "reads every caller"
    #    means, and it is exactly what the hand-kept list did not do.
    for path in sorted(callers):
        mutated = dict(bronnen)
        mutated[path] = callers[path].replace(
            f'System.loadLibrary("{LIB_NAME}")', 'System.loadLibrary("pdf_java")'
        )
        if mutated[path] == callers[path]:
            fail(f"{path} does not load {LIB_NAME} as a literal, so this test cannot "
                "mutate it; check whether the step can")
            continue
        red = run_step(lichaam, mutated)
        if red.returncode == 0:
            fail(f"renaming the library in {path} left the step green; that call "
                "is not covered")
        elif path not in red.stderr:
            fail(f"the step went red for {path} but its message does not name the "
                f"file: {red.stderr.strip()[:200]}")

    # 5. The floor bites in both of its jobs. An empty glob walks the loop zero
    #    times and would otherwise report a pass while reading nothing at all;
    #    and a tree one caller short of the floor is a caller that went missing,
    #    which is the case that has to be reported rather than absorbed.
    empty = run_step(lichaam, {})
    if empty.returncode == 0:
        fail("with no Java sources at all the step passes; an empty glob must be "
            "an error, not a green loop over nothing")
    if vloer and len(callers) >= vloer:
        weg = dict(bronnen)
        del weg[sorted(callers)[0]]
        short = run_step(lichaam, weg)
        if short.returncode == 0:
            fail(f"a tree with {len(callers) - 1} callers passes a floor of {vloer}; "
                "a caller that disappears has to be reported, not absorbed")

    # 6. bindings/java is out of bounds on purpose: its NativeLoader calls
    #    System.loadLibrary(JNI_LIB_NAME) through a constant, and a literal
    #    grep would report that correct code as wrong.
    loader = ROOT / "bindings/java/src/main/java/com/pdfluent/NativeLoader.java"
    if loader.exists():
        with_loader = dict(bronnen)
        with_loader[str(loader.relative_to(ROOT))] = loader.read_text()
        out = run_step(lichaam, with_loader)
        if out.returncode != 0:
            fail("the step trips over bindings/java's NativeLoader, which loads "
                "through a constant; bound the glob to the crate")

    if failures:
        print("[java-name] FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1
    print(f"[java-name] OK: the step reads all {len(callers)} callers under "
          f"{CRATE} against its own floor of {vloer}; renaming any one of them "
          "turns it red, so does losing one, an empty tree is an error, and "
          "NativeLoader's constant is not mistaken for a wrong name.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
