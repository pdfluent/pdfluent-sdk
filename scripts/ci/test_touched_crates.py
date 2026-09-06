#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""What the landing lane compiles, held to its promise (#343).

The selection in touched_crates.py decides how little a landing builds. Getting
it wrong is silent in both directions and expensive in one:

  * TOO LITTLE is the dangerous one. A crate left out of the selection is not
    built and not tested on the machine that is landing, and the summary names
    the crates it DID pick -- so a missing dependent reads exactly like a
    landing that legitimately touched one crate. The whole workspace runs on
    master afterwards, which is where such a break would surface, an hour later
    and attributed to whoever landed next.
  * TOO MUCH costs the minutes this ticket exists to win back, and nothing else.

So every case here asserts membership BOTH ways -- what must be in the selection
and what must not -- over workspaces built for the case rather than over the real
one. A test that only asserted "pdfluent is in it" would pass on a function that
returned every crate every time.

The reverse graph is the half worth the fixtures: it is transitive, it includes
dev-dependencies, and neither property can be read off a single manifest.
"""
from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

CI = pathlib.Path(__file__).resolve().parent
REPO = CI.parents[1]
sys.path.insert(0, str(CI))

import touched_crates as tc  # noqa: E402

fails: list[str] = []
ran = 0


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f"   [{detail}]" if not ok and detail else ""))
    if not ok:
        fails.append(what)


# A workspace on paper: crate -> (directory, the crates it depends on).
#
#   leaf  <- middle <- top          and  tool, which depends on nothing
#
# `top` depends on `middle` only through its dev-dependencies in the fixture
# below, because that is the edge a normal-only graph would drop.
FIXTURE = {
    "leaf": ("crates/leaf", []),
    "middle": ("crates/middle", ["leaf"]),
    "top": ("crates/top", ["middle"]),
    "tool": ("tools/tool", []),
    "pdf-desktop": ("crates/pdf-desktop", ["leaf"]),
}


def fixture() -> tuple[dict[str, str], dict[str, set[str]]]:
    dirs = {name: directory for name, (directory, _) in FIXTURE.items()}
    dependents: dict[str, set[str]] = {name: set() for name in FIXTURE}
    for name, (_, deps) in FIXTURE.items():
        for dep in deps:
            dependents[dep].add(name)
    return dirs, dependents


def cargo_metadata_fixture(root: pathlib.Path) -> dict:
    """A metadata document of the shape `cargo metadata --no-deps` returns.

    Built rather than run, so the graph reading is exercised on a workspace
    whose answers are known -- including the dev-dependency edge, which is what
    `workspace()` must not filter out.
    """
    packages = []
    for name, (directory, deps) in FIXTURE.items():
        packages.append({
            "name": name,
            "manifest_path": str(root / directory / "Cargo.toml"),
            "dependencies": [
                {"name": dep, "kind": "dev" if name == "top" else None} for dep in deps
            ],
        })
    return {"workspace_root": str(root), "packages": packages}


def main() -> int:
    dirs, dependents = fixture()

    # --- the graph, read from a metadata document -------------------------
    root = pathlib.Path("/fixture/ws")
    read_dirs, read_dependents = tc.workspace(cargo_metadata_fixture(root))
    expect("a crate's directory comes out relative to the workspace root",
           read_dirs["middle"] == "crates/middle", read_dirs.get("middle", ""))
    expect("a dependency becomes a reverse edge",
           read_dependents["leaf"] == {"middle", "pdf-desktop"},
           str(read_dependents.get("leaf")))
    expect("a DEV dependency becomes one too",
           read_dependents["middle"] == {"top"}, str(read_dependents.get("middle")))
    expect("a crate nothing depends on has no dependents",
           read_dependents["tool"] == set(), str(read_dependents.get("tool")))

    # --- which crate a file belongs to ------------------------------------
    expect("a source file belongs to its crate",
           tc.crate_of("crates/middle/src/lib.rs", dirs) == "middle")
    expect("a crate's own manifest belongs to it",
           tc.crate_of("crates/middle/Cargo.toml", dirs) == "middle")
    expect("a file outside every crate belongs to none",
           tc.crate_of("docs/ci/runbook.md", dirs) is None)
    # A prefix is not a parent. `crates/middleware/...` starts with
    # `crates/middle` and is a different crate; without the separator it would
    # be built as `middle` and the real crate left out.
    expect("a directory whose name merely starts with another's is not it",
           tc.crate_of("crates/middleware/src/lib.rs",
                       {**dirs, "middleware": "crates/middleware"}) == "middleware")
    # The same shape with the sibling crate absent, which is the one a prefix
    # test gets wrong AND silently: it answers `middle`, so `middleware` is
    # never built and the summary claims a crate that was not changed.
    expect("and a directory the map does not know is nobody's, not its prefix's",
           tc.crate_of("crates/middleware/src/lib.rs", dirs) is None,
           str(tc.crate_of("crates/middleware/src/lib.rs", dirs)))
    expect("so an unknown sibling makes the whole workspace the answer",
           tc.select(["crates/middleware/src/lib.rs"], dirs, dependents) == tc.ALL)
    # The deepest match wins, so a crate nested inside another's directory is
    # attributed to itself.
    nested = {**dirs, "inner": "crates/middle/inner"}
    expect("a nested crate takes its own files",
           tc.crate_of("crates/middle/inner/src/lib.rs", nested) == "inner")

    # --- the selection ----------------------------------------------------
    chosen = tc.select(["crates/leaf/src/lib.rs"], dirs, dependents, excluded=())
    expect("the crate itself is selected", "leaf" in chosen, str(chosen))
    expect("so is its direct dependent", "middle" in chosen, str(chosen))
    expect("and its dependent's dependent, transitively",
           "top" in chosen, str(chosen))
    expect("a crate on no path from it is NOT selected",
           "tool" not in chosen, str(chosen))

    chosen = tc.select(["crates/top/src/main.rs"], dirs, dependents, excluded=())
    expect("selecting a crate does not drag in what it depends on",
           chosen == ["top"], str(chosen))

    chosen = tc.select(["crates/leaf/src/lib.rs"], dirs, dependents)
    expect("an excluded crate is dropped even when it is a dependent",
           "pdf-desktop" not in chosen, str(chosen))
    expect("and dropping it keeps the rest", "middle" in chosen, str(chosen))

    chosen = tc.select(["tools/tool/src/main.rs", "crates/leaf/src/lib.rs"],
                       dirs, dependents, excluded=())
    expect("two crates in one change give the union, sorted",
           chosen == ["leaf", "middle", "pdf-desktop", "tool", "top"], str(chosen))

    # NO CRATES IS AN ANSWER, and the one that wins the minutes back: a change
    # to scripts/ci or to a document compiles nothing.
    expect("a change outside every crate selects nothing",
           tc.select(["scripts/ci/local_ci_gate.sh", "docs/ci/x.md", "README.md"],
                     dirs, dependents, excluded=()) == [], "")

    # --- when the answer has to be everything -----------------------------
    for path in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
                 "rust-toolchain", ".cargo/config.toml"):
        expect(f"`{path}` means the whole workspace",
               tc.select([path], dirs, dependents) == tc.ALL, path)
    # And a crate's own manifest does NOT: that is a change to one crate.
    expect("a crate's Cargo.toml does not mean the whole workspace",
           tc.select(["crates/leaf/Cargo.toml"], dirs, dependents,
                     excluded=()) != tc.ALL)
    # A file under a directory that holds crates, in no crate this metadata
    # knows. The likeliest cause is a crate added by this very change, and
    # answering "no crates" would leave the new one unbuilt.
    expect("an unknown crate directory means the whole workspace",
           tc.select(["crates/brand-new/src/lib.rs"], dirs, dependents) == tc.ALL)
    expect("and so does one under any other directory that holds crates",
           tc.select(["tools/brand-new/src/main.rs"], dirs, dependents) == tc.ALL)

    # --- the duplicated exclude list --------------------------------------
    with tempfile.TemporaryDirectory(prefix="touched-") as d:
        tmp = pathlib.Path(d)
        for name in tc.RUN_SCRIPTS:
            (tmp / name).write_text("cargo test --workspace --exclude pdf-desktop "
                                    "--exclude xfa-wasm\n")
        expect("agreeing run scripts raise nothing", tc.excludes_agree(tmp) == [],
               str(tc.excludes_agree(tmp)))
        (tmp / "run_test.sh").write_text("cargo test --workspace --exclude pdf-desktop\n")
        problems = tc.excludes_agree(tmp)
        expect("a run script that drops an exclude is reported",
               len(problems) == 1 and "run_test.sh" in problems[0], str(problems))
        (tmp / "run_test.sh").unlink()
        expect("a missing run script is reported rather than passed over",
               any("run_test.sh" in p for p in tc.excludes_agree(tmp)),
               str(tc.excludes_agree(tmp)))

    # --- and the real workspace, end to end -------------------------------
    #
    # The fixtures prove the reasoning; this proves the reasoning is pointed at
    # this repository. It runs the script the way the gate runs it.
    def run(*args: str) -> tuple[int, list[str]]:
        out = subprocess.run([sys.executable, str(CI / "touched_crates.py"), *args],
                             capture_output=True, text=True, cwd=str(REPO))
        return out.returncode, [r for r in out.stdout.splitlines() if r]

    rc, lines = run("--file", "scripts/ci/local_ci_gate.sh")
    expect("a scripts-only change compiles no crate here", rc == 0 and lines == [],
           f"rc={rc} {lines}")

    # `crates/xfa-license/src/lib.rs` stood here until 06-09-2026. That crate was
    # deleted with the licence key it validated (#226), so the case stopped
    # measuring what it was written for: an unknown directory is answered with
    # the whole workspace, and `['*']` is a legitimate answer that happens not to
    # contain "xfa-license". The two assertions failed for the one reason a
    # fixture-backed test cannot see -- its subject was gone.
    #
    # Any crate with a dependent does. pdf-manip is picked because pdfluent
    # depends on it, neither is on the CI exclude list, and its directory name
    # and package name are the same string -- pdf-forms publishes as
    # `pdfluent-forms`, and the selection speaks package names, so that one
    # would have tested the rename rather than the selection.
    rc, lines = run("--file", "crates/pdf-manip/src/lib.rs")
    expect("a real crate names itself", rc == 0 and "pdf-manip" in lines,
           f"rc={rc} {lines[:5]}")
    expect("and pulls in a real dependent", "pdfluent" in lines, str(lines[:8]))
    expect("without naming the crates CI excludes",
           not (set(lines) & set(tc.EXCLUDED)), str(lines))

    rc, lines = run("--file", "Cargo.lock")
    expect("the real lockfile still means everything",
           rc == 0 and lines == [tc.ALL], f"rc={rc} {lines}")

    rc, _ = run("--check-excludes")
    expect("the exclude lists in this repository agree", rc == 0, str(rc))

    print(f"[touched-crates-test] {ran - len(fails)}/{ran} case(s) ok")
    if fails:
        print("[touched-crates-test] FAIL: " + "; ".join(fails))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
