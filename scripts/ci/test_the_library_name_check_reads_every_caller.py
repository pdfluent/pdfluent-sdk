#!/usr/bin/env python3
"""The Java library-name check must read every caller, not a list of three.

The step in java-bindings.yml exists because Cargo.toml built libpdfluent_java
while the Java side loaded pdf_java. It guarded that with three file paths typed
out by hand, while five files under crates/pdf-java call System.loadLibrary --
and two of the uncovered ones arrived in the very change that added the check.
A list kept by hand drifts away from the tree silently, which is the same shape
of failure the step was written to catch.

So this test does not paraphrase the check: it lifts the step's own `run` body
out of the workflow and executes it against trees it builds, including one tree
per caller with that caller's name changed. If any single call could be renamed
without the step noticing, one of those runs comes back green and this test
fails.
"""
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

import yaml

WORTEL = pathlib.Path(__file__).resolve().parents[2]
WORKFLOW = WORTEL / ".github/workflows/java-bindings.yml"
CRATE = "crates/pdf-java"
NAAM = "pdfluent_java"
# Five today. A floor, not an equality: adding a caller is normal, and the
# check is meant to cover it rather than to forbid it.
VLOER = 5

fouten: list[str] = []


def mis(regel: str) -> None:
    fouten.append(regel)


def staplichaam() -> str | None:
    """The shipped text of the name-checking step, or None if it is gone.

    Recognised by its subject -- a run body that mentions System.loadLibrary --
    and not by any variable this fix happens to use. Keyed on `loaders`, this
    would report a hand-listed step as no step at all, and the check for a
    returning hand-list below would never run.
    """
    doc = yaml.safe_load(WORKFLOW.read_text())
    kandidaten = [
        stap["run"]
        for job in doc.get("jobs", {}).values()
        for stap in (job.get("steps", []) or [])
        if "System.loadLibrary" in (stap.get("run") or "")
    ]
    if len(kandidaten) > 1:
        mis(f"{len(kandidaten)} steps check the loaded library name; this test "
            "reads one of them and would miss a regression in the others")
    return kandidaten[0] if kandidaten else None


def bouw_boom(basis: pathlib.Path, bestanden: dict[str, str]) -> None:
    """A tree the step can run in: the sources, plus the .so it tests for."""
    gebouwd = basis / "target/debug"
    gebouwd.mkdir(parents=True, exist_ok=True)
    (gebouwd / f"lib{NAAM}.so").write_bytes(b"")
    for pad, inhoud in bestanden.items():
        doel = basis / pad
        doel.parent.mkdir(parents=True, exist_ok=True)
        doel.write_text(inhoud)


def draai(lichaam: str, bestanden: dict[str, str]) -> subprocess.CompletedProcess:
    with tempfile.TemporaryDirectory() as tmp:
        basis = pathlib.Path(tmp)
        bouw_boom(basis, bestanden)
        return subprocess.run(
            ["bash", "-c", lichaam],
            cwd=basis,
            capture_output=True,
            text=True,
        )


def echte_bronnen() -> dict[str, str]:
    """Every .java file under the crate, as it stands in the repository."""
    return {
        str(p.relative_to(WORTEL)): p.read_text()
        for p in sorted((WORTEL / CRATE).rglob("*.java"))
    }


def main() -> int:
    if not WORKFLOW.exists():
        print(f"[java-naam] FAIL: {WORKFLOW.relative_to(WORTEL)} is gone")
        return 1

    lichaam = staplichaam()
    if lichaam is None:
        print("[java-naam] FAIL: no step in java-bindings.yml checks the loaded "
              "library name; the name went out of step once already")
        return 1

    # 1. The list is derived, not typed. A `for f in .../Foo.java \` list is
    #    the thing being removed; catching its return is the point.
    if re.search(r"for f in\s+\S*/\S+\.java", lichaam):
        mis("the step enumerates .java paths by hand again; derive them with a "
            "glob over the crate instead")
    if f"{CRATE} --include" not in lichaam and f"{CRATE}' --include" not in lichaam:
        mis(f"the step does not glob over {CRATE}")

    bronnen = echte_bronnen()
    aanroepers = {p: t for p, t in bronnen.items() if "System.loadLibrary" in t}

    # 2. The premise: there really are more callers than the old list named.
    if len(aanroepers) < VLOER:
        mis(f"expected at least {VLOER} System.loadLibrary callers under "
            f"{CRATE}, found {len(aanroepers)}: {sorted(aanroepers)}")

    # 3. The tree as it stands passes. Without this the red runs below prove
    #    only that the step can fail, not that it can tell the cases apart.
    schoon = draai(lichaam, bronnen)
    if schoon.returncode != 0:
        mis(f"the step fails on the repository as it stands (rc="
            f"{schoon.returncode}): {schoon.stderr.strip()[:400]}")

    # 4. The mutation, one caller at a time. Every one of them must be able to
    #    turn the step red on its own -- that is what "reads every caller"
    #    means, and it is exactly what the hand-kept list did not do.
    for pad in sorted(aanroepers):
        gemuteerd = dict(bronnen)
        gemuteerd[pad] = aanroepers[pad].replace(
            f'System.loadLibrary("{NAAM}")', 'System.loadLibrary("pdf_java")'
        )
        if gemuteerd[pad] == aanroepers[pad]:
            mis(f"{pad} does not load {NAAM} as a literal, so this test cannot "
                "mutate it; check whether the step can")
            continue
        rood = draai(lichaam, gemuteerd)
        if rood.returncode == 0:
            mis(f"renaming the library in {pad} left the step green; that call "
                "is not covered")
        elif pad not in rood.stderr:
            mis(f"the step went red for {pad} but its message does not name the "
                f"file: {rood.stderr.strip()[:200]}")

    # 5. The floor. An empty glob walks the loop zero times and would otherwise
    #    report a pass while reading nothing at all.
    leeg = draai(lichaam, {})
    if leeg.returncode == 0:
        mis("with no Java sources at all the step passes; an empty glob must be "
            "an error, not a green loop over nothing")

    # 6. bindings/java is out of bounds on purpose: its NativeLoader calls
    #    System.loadLibrary(JNI_LIB_NAME) through a constant, and a literal
    #    grep would report that correct code as wrong.
    lader = WORTEL / "bindings/java/src/main/java/com/pdfluent/NativeLoader.java"
    if lader.exists():
        met_lader = dict(bronnen)
        met_lader[str(lader.relative_to(WORTEL))] = lader.read_text()
        uit = draai(lichaam, met_lader)
        if uit.returncode != 0:
            mis("the step trips over bindings/java's NativeLoader, which loads "
                "through a constant; bound the glob to the crate")

    if fouten:
        print("[java-naam] FAIL")
        for f in fouten:
            print(f"  - {f}")
        return 1
    print(f"[java-naam] OK: the step reads all {len(aanroepers)} callers under "
          f"{CRATE}; renaming any one of them turns it red, an empty tree is an "
          "error, and NativeLoader's constant is not mistaken for a wrong name.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
