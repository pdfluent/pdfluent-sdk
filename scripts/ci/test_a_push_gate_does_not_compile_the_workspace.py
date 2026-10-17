#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Drives a_push_gate_does_not_compile_the_workspace.py over trees it must
refuse and trees it must not. (#473)

The guard's failure mode is the one every guard in this directory has: being
green because it stopped looking. There are four ways for that to happen here
and each has a case below.

  the compiler moved one file deeper      a gate calls a script that calls cargo
  the compiler moved two files deeper     and that script calls another
  the path was built from constants       `subprocess.run(["bash", str(HELPER)])`
                                          is how half this directory names the
                                          script it drives
  the lane stopped meaning what it says   a `zwaar` that no longer defers puts
                                          every heavy gate back on every push,
                                          and a guard reading the `run` lines
                                          would report a clean lane

And the opposite mistake, which is the reason the guard reads a syntax tree
rather than the text: nine guards in scripts/ci carry `cargo test` inside a
fixture string or a comment, and one of them is read by the fast lane on every
push. A guard that reported those would be switched off in a week.

Every case runs the guard in a throwaway tree holding nothing but a gate script,
so no case can be answered by this repository's real one.
"""
from __future__ import annotations

import importlib.util
import io
import pathlib
import subprocess
import sys
import tempfile
from contextlib import redirect_stdout

ROOT = pathlib.Path(__file__).resolve().parents[2]
GUARD = ROOT / "scripts" / "ci" / "a_push_gate_does_not_compile_the_workspace.py"
GATE = ROOT / "scripts" / "ci" / "local_ci_gate.sh"
failures: list[str] = []
done = 0


def expect(what: str, ok: bool, detail: str = "") -> None:
    global done
    done += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f"   [{detail}]" if not ok and detail else ""))
    if not ok:
        failures.append(what)


# The helpers, in the shape the guard reads them: the three that defer say so in
# their body. Written out here rather than copied from the real gate, because a
# fixture that copied the file it is testing would pass on a file that had
# stopped deferring.
HELPERS = """#!/usr/bin/env bash
set -uo pipefail
run() { local name="$1"; shift; "$@"; }
zwaar() { local name="$1"; shift
  if [ "${LANDING:-0}" = 1 ]; then run "$name" "$@"
  else echo "=== $name DEFERRED (not a pass)"; fi
}
crate_gate() { local name="$1" crate="$2"; shift 2
  if [ "${LANDING:-0}" = 1 ]; then run "$name" "$@"
  else echo "=== $name DEFERRED (not a pass)"; fi
}
scoped() { local name="$1"; shift
  if [ "${LANDING:-0}" = 1 ]; then run "$name" "$@"
  else echo "=== $name DEFERRED (not a pass)"; fi
}
"""

# Enough gates to clear the guard's floor, which exists so that a broken scan
# refuses instead of approving an empty list.
PADDING = "\n".join(f"run pad{n:03d} python3 scripts/ci/nothing.py" for n in range(80))


class Verdict:
    """What a subprocess would have handed back, without the subprocess.

    Twenty-four fixtures at a fifth of a second of interpreter start-up each is
    five seconds on the push lane, spent by the test of the guard that exists to
    take seventy-four seconds off that same lane. So the guard is imported from
    the fixture tree and its main() called: it computes its own root from
    __file__, which is the copy in the fixture, so a run in-process reads
    exactly the tree a run out-of-process would have. The real tree is still
    judged by running the file as a script, at the bottom, because "it works
    when imported" is not the claim the lane makes.
    """

    def __init__(self, returncode: int, stdout: str) -> None:
        self.returncode, self.stdout = returncode, stdout


def judge(guard: pathlib.Path) -> Verdict:
    spec = importlib.util.spec_from_file_location(f"guard_{id(guard)}", guard)
    module = importlib.util.module_from_spec(spec)
    caught = io.StringIO()
    with redirect_stdout(caught):
        spec.loader.exec_module(module)
        code = module.main()
    return Verdict(code, caught.getvalue())


def tree(lines: str, files: dict[str, str] | None = None, helpers: str = HELPERS) -> Verdict:
    """Run the guard against a gate script made of `lines`, in a fresh tree."""
    with tempfile.TemporaryDirectory() as temporary:
        fake = pathlib.Path(temporary)
        (fake / "scripts" / "ci").mkdir(parents=True)
        (fake / "scripts" / "ci" / "a_push_gate_does_not_compile_the_workspace.py").write_text(
            GUARD.read_text())
        (fake / "scripts" / "ci" / "local_ci_gate.sh").write_text(
            helpers + "\n" + PADDING + "\n" + lines + "\n")
        (fake / "scripts" / "ci" / "nothing.py").write_text("print('nothing')\n")
        for name, body in (files or {}).items():
            path = fake / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(body)
        return judge(fake / "scripts" / "ci" / "a_push_gate_does_not_compile_the_workspace.py")


print("a clean push lane passes")
clean = tree("run guard python3 scripts/ci/nothing.py")
expect("a lane of file readers passes", clean.returncode == 0, clean.stdout[-300:])
expect("and it says how many gates it read", "gates on the push" in clean.stdout, clean.stdout[-200:])

print("a compiler on the push lane is refused")
inline = tree("run compile cargo test -p pdfluent")
expect("`run x cargo test` is refused", inline.returncode == 1, f"rc={inline.returncode}")
expect("and the gate is named", "compile" in inline.stdout, inline.stdout[-300:])
for subcommand in ("build", "check", "run", "clippy", "bench", "rustc"):
    got = tree(f"run compile cargo {subcommand} -p pdfluent")
    expect(f"`cargo {subcommand}` is refused too", got.returncode == 1, f"rc={got.returncode}")

print("and the two cargo subcommands that compile nothing are not")
for harmless in ("metadata --no-deps", "fmt --all -- --check", "tree --target wasm32-unknown-unknown"):
    got = tree(f"run cheap cargo {harmless}")
    expect(f"`cargo {harmless.split()[0]}` still passes", got.returncode == 0, got.stdout[-200:])

print("the landing lane may compile, which is the whole point of moving it there")
for helper in ("zwaar heavy cargo test -p pdfluent",
               "crate_gate heavy pdfluent cargo build -q --examples -p pdfluent",
               "scoped heavy bash scripts/ci/compiles.sh"):
    got = tree(helper, {"scripts/ci/compiles.sh": "#!/usr/bin/env bash\ncargo build -p pdfluent\n"})
    expect(f"`{helper.split()[0]}` is allowed to compile", got.returncode == 0, got.stdout[-300:])

print("one file deeper")
one_hop = tree("run wasm bash scripts/ci/compiles.sh",
               {"scripts/ci/compiles.sh": "#!/usr/bin/env bash\ncargo check --target wasm32-unknown-unknown\n"})
expect("a gate whose script compiles is refused", one_hop.returncode == 1, one_hop.stdout[-300:])
expect("and the line is named", "compiles.sh:2" in one_hop.stdout, one_hop.stdout[-300:])

print("two files deeper")
two_hops = tree("run wasm bash scripts/ci/outer.sh", {
    "scripts/ci/outer.sh": "#!/usr/bin/env bash\npython3 scripts/ci/inner.py\n",
    "scripts/ci/inner.py": "import subprocess\nsubprocess.run(['cargo', 'build', '-p', 'pdfluent'])\n",
})
expect("a compiler two hops away is refused", two_hops.returncode == 1, two_hops.stdout[-300:])
expect("and the chain is printed", "outer.sh" in two_hops.stdout and "inner.py" in two_hops.stdout,
       two_hops.stdout[-300:])

print("a path built from constants is still a path")
folded = tree("run fixture python3 scripts/ci/driver.py", {
    "scripts/ci/driver.py": (
        "import pathlib, subprocess\n"
        "ROOT = pathlib.Path(__file__).resolve().parents[2]\n"
        "CI = ROOT / 'scripts' / 'ci'\n"
        "HELPER = CI / 'helper.sh'\n"
        "subprocess.run(['bash', str(HELPER)], check=True)\n"
    ),
    "scripts/ci/helper.sh": "#!/usr/bin/env bash\ncargo test -p pdfluent\n",
})
expect("a script named by a folded constant is followed", folded.returncode == 1, folded.stdout[-400:])

print("prose is not a compiler")
prose = tree("run reader python3 scripts/ci/reads.py", {
    "scripts/ci/reads.py": (
        '"""This guard checks that `cargo test -p pdf-ocr --features tesseract` runs."""\n'
        "# cargo build --release would be wrong here\n"
        "WANTED = 'cargo test --workspace'\n"
        "print(WANTED)\n"
    ),
})
expect("a fixture string naming cargo is not a compile", prose.returncode == 0, prose.stdout[-300:])

shell_prose = tree("run reader bash scripts/ci/reads.sh", {
    "scripts/ci/reads.sh": "#!/usr/bin/env bash\n# cargo build -p pdfluent\ngrep -q 'cargo test' Makefile\n",
})
expect("a shell comment naming cargo is not a compile", shell_prose.returncode == 0,
       shell_prose.stdout[-300:])

print("a guard that READS the gate script is not running it")
reader = tree("run gateread python3 scripts/ci/reads_gate.py", {
    "scripts/ci/reads_gate.py": (
        "import pathlib\n"
        "print(pathlib.Path('scripts/ci/local_ci_gate.sh').read_text()[:10])\n"
    ),
}, helpers=HELPERS)
expect("reading local_ci_gate.sh does not import its landing lane",
       reader.returncode == 0, reader.stdout[-400:])

print("the lane has to still mean what it says")
without = HELPERS.replace('else echo "=== $name DEFERRED (not a pass)"; fi\n}\ncrate_gate',
                          'else run "$name" "$@"; fi\n}\ncrate_gate')
drifted = tree("run guard python3 scripts/ci/nothing.py", helpers=without)
expect("a `zwaar` that stopped deferring is a refusal, not a pass", drifted.returncode == 2,
       f"rc={drifted.returncode}: {drifted.stdout[-300:]}")
expect("and it says what that would cost", "every gate it carries is on every push" in drifted.stdout,
       drifted.stdout[-300:])

print("an empty scan approves nothing")
empty = tree("run guard python3 scripts/ci/nothing.py").returncode
with tempfile.TemporaryDirectory() as temporary:
    fake = pathlib.Path(temporary)
    (fake / "scripts" / "ci").mkdir(parents=True)
    (fake / "scripts" / "ci" / "a_push_gate_does_not_compile_the_workspace.py").write_text(GUARD.read_text())
    (fake / "scripts" / "ci" / "local_ci_gate.sh").write_text(HELPERS + "\nrun one python3 x.py\n")
    thin = judge(fake / "scripts" / "ci" / "a_push_gate_does_not_compile_the_workspace.py")
expect("a gate list too short to be the real one is refused", thin.returncode == 2,
       f"rc={thin.returncode}: {thin.stdout[-200:]}")
expect("and it says a scan that finds nothing approves everything",
       "approves everything" in thin.stdout, thin.stdout[-200:])
expect("(control) the same fixture with a full list passes", empty == 0)

print("a python file that does not parse is a finding, not a shrug")
broken = tree("run broken python3 scripts/ci/broken.py",
              {"scripts/ci/broken.py": "def x(:\n    pass\n"})
expect("an unparsable script on the lane is reported", broken.returncode == 1, broken.stdout[-300:])

print("and this repository obeys the rule")
real = subprocess.run([sys.executable, str(GUARD)], cwd=ROOT, capture_output=True, text=True, timeout=300)
expect("the real fast lane reaches no compiler", real.returncode == 0, real.stdout[-600:])
expect("the real gate script still has the four helpers",
       all(f"\n{helper}" in GATE.read_text() for helper in ("run()", "zwaar()", "crate_gate()", "scoped()")),
       "local_ci_gate.sh")
expect("(control) it is run as a script here, not only imported",
       "a_push_gate_does_not_compile_the_workspace" in real.stdout, real.stdout[-200:])

print(f"test_a_push_gate_does_not_compile_the_workspace: {done} case(s), {len(failures)} failure(s)")
sys.exit(1 if failures else 0)
