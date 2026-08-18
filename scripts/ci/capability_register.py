#!/usr/bin/env python3
"""The single register of what this product can do, and in what state.

WHY THIS EXISTS

We were repeatedly wrong about our own product, in both directions. PDF/UA was
planned as a two-week build while pdf-compliance already held ~29,000 lines of
it. Text replacement was written up as missing after it had shipped. Excel and
PowerPoint were advertised while unreachable from the facade. Each mistake was
made against a hand-written description that had gone stale without saying so.

Three such descriptions were four months out of date at the same moment, and all
three still read as authoritative. The lesson is not "write better documents" —
it is that any hand-maintained account of a moving codebase is wrong within
weeks, and nothing tells you which weeks.

So this file is generated from the code, and `--check` fails the build when the
committed copy no longer matches. It answers, for every capability, five
questions that were previously scattered across greps, memory and hope:

    1. What does it do?                 (the promise we make)
    2. Where is it implemented?         (crate + defining file)
    3. Can a customer reach it?         (cargo tree, not a guess)
    4. Which test covers it?            (a test that calls it)
    5. WHICH CI JOB RUNS THAT TEST?     (the one nobody was asking)

Question 5 is the point. Until 2026-08-18 the workspace suite -- 202 test
binaries covering merge, split, compress, password protection, watermarking and
every Office conversion -- was manual on merge requests, manual on master, and
automatic only on a schedule that did not exist. The tests were fine. Nothing
ran them, for months, and every report said "tested". A test that no job
executes is not coverage; it is a file. This register refuses to call such a
capability tested, which is the Definition of Done in CLAUDE.md made mechanical.

Usage:
    capability_register.py            # regenerate docs/CAPABILITY_REGISTER.md
    capability_register.py --check    # fail if the committed copy is stale

Exit codes:
    0  written, or up to date
    1  --check and the committed copy is stale
    2  could not run
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
OUT = REPO / "docs" / "CAPABILITY_REGISTER.md"

sys.path.insert(0, str(REPO / "scripts" / "ci"))
from feature_promises import PROMISES  # noqa: E402  single source of truth for promises
from api_coverage import BINDINGS, impl_blocks  # noqa: E402  one definition of what each binding exports

# Bodies that mean "this method exists but does nothing at runtime".
STUB_MARKERS = ("MissingDependency", "UnsupportedOnWasm", "todo!", "unimplemented!")

# Crates the workspace suite deliberately skips, and what covers them instead.
# `cargo test --workspace --exclude pdf-desktop --exclude xfa-wasm` in
# scripts/ci/run_test.sh -- keep this in step with that line.
WORKSPACE_SUITE = "quality:cargo-test"
EXCLUDED_FROM_SUITE = {
    "xfa-wasm": "sanity:wasm-binding-smoke",
    "pdf-desktop": None,  # nothing runs it
}


def run(cmd: list[str]) -> str:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True).stdout


# --------------------------------------------------------------------------- #
# Sources of truth
# --------------------------------------------------------------------------- #

def workspace_crates() -> list[dict]:
    meta = json.loads(run(["cargo", "metadata", "--format-version", "1", "--no-deps"]) or "{}")
    return sorted(meta.get("packages", []), key=lambda p: p["name"])


def facade_deps() -> set[str]:
    """Crates reachable from the facade. `cargo tree`, never a Cargo.toml grep:
    dependencies live in several sections and some are platform-gated, which is
    how docx was once reported as unreachable when it was not."""
    tree = run(["cargo", "tree", "-p", "pdfluent", "--depth", "1", "--edges", "normal"])
    return {m.group(1) for m in re.finditer(r"^[│├└─\s]*([a-z0-9_-]+) v", tree, re.M)}


def facade_methods() -> dict[str, bool]:
    """name -> is_stub, for every public method on Document."""
    src = (REPO / "crates" / "pdfluent" / "src" / "document.rs").read_text(errors="replace")
    out: dict[str, bool] = {}
    for m in re.finditer(r"pub fn (\w+)", src):
        name = m.group(1)
        body = src[m.end() : m.end() + 900].split("\n    pub fn ")[0]
        stub = any(k in body for k in STUB_MARKERS)
        # A method with both a real arm and a wasm-stub arm counts as real.
        out[name] = out.get(name, True) and stub
    return out


def rust_sources() -> dict[Path, str]:
    out = {}
    for pattern in ("crates/*/tests/*.rs", "crates/*/src/**/*.rs"):
        for f in REPO.glob(pattern):
            if "/target/" in str(f):
                continue
            out[f] = f.read_text(errors="replace")
    return out


def targeted_test_jobs() -> dict[str, list[str]]:
    """test-binary name -> CI jobs that run it by name (`cargo test --test X`).

    These matter because they run outside the big suite, often on a different
    trigger, so a capability can be covered by a fast gate even when the slow
    suite has not run yet."""
    ci = (REPO / ".gitlab-ci.yml").read_text(errors="replace")
    out: dict[str, list[str]] = {}
    job = None
    for line in ci.splitlines():
        m = re.match(r"^([a-zA-Z_.][\w:.\-]*):\s*$", line)
        if m:
            job = m.group(1)
        if job and not job.startswith("."):
            for t in re.findall(r"--test\s+([\w-]+)", line):
                out.setdefault(t, []).append(job)
    return {k: sorted(set(v)) for k, v in out.items()}


def binding_test_jobs() -> dict[str, str]:
    """Path fragment -> the CI job that runs tests under it."""
    return {
        "crates/pdf-python/tests": "test:binding-python",
        "bindings/node": "test:binding-node",
        "bindings/java": "test:binding-java",
        "bindings/dotnet": "test:binding-dotnet",
        "crates/xfa-wasm": "sanity:wasm-binding-smoke",
    }


def empty_features() -> list[str]:
    t = (REPO / "crates" / "pdfluent" / "Cargo.toml").read_text(errors="replace")
    m = re.search(r"\[features\](.*?)(\n\[|\Z)", t, re.S)
    return [
        l.split("=")[0].strip()
        for l in (m.group(1) if m else "").splitlines()
        if "=" in l and l.split("=", 1)[1].strip() in ("[]", "[ ]")
    ]


def binding_exports() -> dict[str, set[str]]:
    """binding label -> every name it exports, in both spellings.

    Reuses api_coverage's patterns rather than writing new ones: the last time
    this was done by hand, four of the five bindings were reported near zero
    because JNI symbols are `Java_<pkg>_<Class>_<method>` and napi renames
    snake_case to camelCase. One definition, used twice, cannot drift into two
    different wrong answers.
    """
    out: dict[str, set[str]] = {}
    for b in BINDINGS:
        names: set[str] = set()
        text = ""
        for g in b.src_globs:
            for f in REPO.glob(g):
                text += f.read_text(errors="replace")
        # Methods inside #[pymethods] / #[napi] / #[wasm_bindgen] impl blocks carry
        # no per-function attribute, so a pattern-only scan misses them entirely.
        # Merge, split and compress are all pyo3 *methods*: without this, Python
        # reads as not exposing capabilities it has exposed since day one.
        scan = text
        for block in impl_blocks(text, b.impl_attrs):
            for m in re.finditer(r"pub (?:async )?fn (\w+)", block):
                names.add(m.group(1))
        for pattern, group in b.export_patterns:
            for m in re.finditer(pattern, scan):
                raw = m.group(group)
                names.add(raw)
                # JNI: Java_com_pdfluent_Doc_mergePdfs -> mergePdfs
                if raw.startswith("Java_"):
                    names.add(raw.rsplit("_", 1)[-1])
                # camelCase <-> snake_case, because the wrappers rename.
                names.add(re.sub(r"([A-Z])", lambda x: "_" + x.group(1).lower(), raw).lstrip("_"))
        out[b.name] = names
    return out


def exposes(names: set[str], symbols: list[str]) -> bool:
    for s_ in symbols:
        camel = re.sub(r"_(\w)", lambda m: m.group(1).upper(), s_)
        if s_ in names or camel in names:
            return True
        # A binding often prefixes: pdfluent_merge_pdfs, doc_merge_pdfs.
        if any(n.endswith("_" + s_) or n.endswith(camel[0].upper() + camel[1:]) for n in names):
            return True
    return False


# --------------------------------------------------------------------------- #
# Derivation
# --------------------------------------------------------------------------- #

def ci_job_for(test_path: Path) -> str | None:
    """Which CI job executes this test file? None means nothing does."""
    rel = str(test_path.relative_to(REPO))
    for frag, job in binding_test_jobs().items():
        if rel.startswith(frag):
            return job
    parts = Path(rel).parts
    crate = parts[1] if len(parts) > 2 and parts[0] == "crates" else None
    if crate in EXCLUDED_FROM_SUITE:
        return EXCLUDED_FROM_SUITE[crate]
    if "/tests/" in rel or "/src/" in rel:
        return WORKSPACE_SUITE
    return None


def analyse() -> dict:
    sources = rust_sources()
    tests_only = {
        f: t for f, t in sources.items()
        if "/tests/" in str(f) or "#[test]" in t or "#[wasm_bindgen_test]" in t
    }
    deps = facade_deps()
    methods = facade_methods()
    targeted = targeted_test_jobs()
    bexp = binding_exports()

    rows = []
    for feature, (blurb, crates, symbols) in PROMISES.items():
        # Where is it implemented? The file that defines the symbol.
        impl_files = []
        for f, text in sources.items():
            rel = f.relative_to(REPO)
            if "/tests/" in str(rel) or not any(p == c for c in crates for p in rel.parts):
                continue
            if any(re.search(rf"pub (?:async )?fn {re.escape(s)}\b", text) for s in symbols):
                impl_files.append(str(rel))

        # Which tests call it, and what runs them?
        test_files, jobs = [], set()
        for f, text in tests_only.items():
            rel = f.relative_to(REPO)
            if not any(p == c for c in crates for p in rel.parts):
                continue
            if any(re.search(rf"\b{re.escape(s)}\s*\(", text) for s in symbols):
                test_files.append(str(rel))
                job = ci_job_for(f)
                if job:
                    jobs.add(job)
                # A file also run by name gets that faster gate credited too.
                jobs.update(targeted.get(f.stem, []))

        impl = [c for c in crates if c != "pdfluent"]
        reachable = (not impl) or any(c in deps for c in impl)

        if not test_files:
            state = "UNTESTED"
        elif not jobs:
            state = "NO CI JOB"
        elif not reachable:
            state = "UNREACHABLE"
        else:
            state = "shipped"

        bindings = [b for b, names in bexp.items() if exposes(names, symbols)]

        rows.append({
            "feature": feature, "blurb": blurb, "crates": crates,
            "bindings": bindings,
            "impl": sorted(set(impl_files)), "tests": sorted(set(test_files)),
            "jobs": sorted(jobs), "reachable": reachable, "state": state,
        })

    # Crate-level view: everything published, whether reachable, whether tested.
    crate_rows = []
    for c in workspace_crates():
        if c.get("publish") == []:
            continue
        name = c["name"]
        # Integration tests AND unit tests. Counting only tests/*.rs reported
        # pdf-font, pdf-standard-fonts and five others as having no tests at
        # all, when their tests live in `mod tests` inside src/ -- the same
        # shape of measurement bug that once put the Java binding at 0%. A
        # suspiciously low number is a bug in the ruler far more often than it
        # is news about the code.
        #
        # The directory comes from the manifest path, NEVER from the package
        # name. `pdfluent-forms` lives in crates/pdf-forms and `pdfluent-sign`
        # in crates/pdf-sign: the published name and the directory differ for
        # every renamed crate. Deriving the path from the name reported forms,
        # signing and lopdf as completely untested while all three have full
        # suites -- a false alarm on two capabilities we sell.
        root = Path(c["manifest_path"]).parent
        tdir = root / "tests"
        integration = sorted(p.name for p in tdir.glob("*.rs")) if tdir.is_dir() else []
        unit = sum(
            src.read_text(errors="replace").count("#[test]")
            for src in (root / "src").rglob("*.rs")
            if "/target/" not in str(src)
        ) if (root / "src").is_dir() else 0
        has_tests = bool(integration) or unit > 0
        job = EXCLUDED_FROM_SUITE.get(name, WORKSPACE_SUITE) if has_tests else None
        crate_rows.append({
            "name": name, "version": c["version"],
            "reachable": name in deps or name == "pdfluent",
            "tests": len(integration), "unit": unit, "job": job,
        })

    return {
        "rows": rows, "crates": crate_rows, "methods": methods,
        "empty_features": empty_features(),
    }


# --------------------------------------------------------------------------- #
# Rendering
# --------------------------------------------------------------------------- #

def render(a: dict) -> str:
    rows, crates, methods = a["rows"], a["crates"], a["methods"]
    stubs = sorted(n for n, s in methods.items() if s)
    L: list[str] = []

    def w(s: str = "") -> None:
        L.append(s)

    w("# Capability register")
    w()
    w("**Generated** by `scripts/ci/capability_register.py`. Do not edit by hand —")
    w("`sanity:capability-register` regenerates it and fails the build if this file")
    w("has drifted from the code. That gate is the entire value: three hand-written")
    w("descriptions of this product were four months stale at the same time, and each")
    w("one still read as fact.")
    w()
    w("This is the one place to look before claiming what exists, estimating a build,")
    w("or answering \"do we have X?\". It is current by construction; your memory and")
    w("mine are not.")
    w()

    ok = sum(1 for r in rows if r["state"] == "shipped")
    w("## Summary")
    w()
    w(f"- **{ok} of {len(rows)}** advertised capabilities are implemented, reachable,")
    w("  tested, and covered by a CI job that actually runs the test.")
    w(f"- **{len(methods)}** public methods on `pdfluent::Document`, of which")
    w(f"  **{len(stubs)}** fail at runtime.")
    w(f"- **{sum(1 for c in crates if not c['reachable'])}** published crates cannot be")
    w("  reached from the facade.")
    w(f"- **{sum(1 for c in crates if not c['tests'] and not c['unit'])}** published")
    w("  crates have no tests of any kind.")
    w(f"- **{sum(1 for c in crates if (c['tests'] or c['unit']) and not c['job'])}** crates")
    w("  have tests that no CI job executes.")
    w()

    w("## What the states mean")
    w()
    w("| state | meaning |")
    w("|---|---|")
    w("| `shipped` | implemented, reachable from the facade, tested, and a CI job runs that test |")
    w("| `UNREACHABLE` | built and tested, but a customer using the `pdfluent` crate cannot call it |")
    w("| `NO CI JOB` | a test exists and nothing executes it — this reads as tested and is not |")
    w("| `UNTESTED` | no test calls it |")
    w()
    w("`NO CI JOB` deserves the shouting. It is indistinguishable from `shipped` in")
    w("every report that counts test files instead of test runs, which is how the")
    w("whole workspace suite sat unexecuted for months while every summary said the")
    w("features were covered.")
    w()

    w("## Advertised capabilities")
    w()
    for r in rows:
        w(f"### {r['feature']} — `{r['state']}`")
        w()
        w(f"*{r['blurb']}*")
        w()
        w("| | |")
        w("|---|---|")
        w(f"| **Implemented in** | {', '.join(f'`{c}`' for c in r['crates'])} |")
        if r["impl"]:
            w(f"| **Defined at** | {' · '.join(f'`{p}`' for p in r['impl'][:4])} |")
        else:
            w("| **Defined at** | *(no `pub fn` for the listed symbols found in those crates)* |")
        w(f"| **Reachable from facade** | {'yes' if r['reachable'] else '**no**'} |")
        w(f"| **Exposed in bindings** | {' · '.join(r['bindings']) or '**Rust only**'} |")
        w(f"| **Tested by** | {' · '.join(f'`{p}`' for p in r['tests'][:5]) or '**nothing**'} |")
        w(f"| **Run in CI by** | {' · '.join(f'`{j}`' for j in r['jobs']) or '**no job**'} |")
        w()

    # ---- the action list -------------------------------------------------- #
    rust_only = [r["feature"] for r in rows if not r["bindings"]]
    unreachable = [c["name"] for c in crates if not c["reachable"]]
    bad_state = [r for r in rows if r["state"] != "shipped"]

    w("## Gaps worth acting on")
    w()
    w("Everything below is derived, so this list shortens only when the code changes")
    w("-- not when someone decides it is fine.")
    w()
    if bad_state:
        w("### Capabilities not fully delivered")
        w()
        for r in bad_state:
            w(f"- **{r['feature']}** — `{r['state']}`")
        w()
    if rust_only:
        w("### Advertised in the SDK, available only in Rust")
        w()
        w("No binding exports these, so a Python, Node, Java, .NET or WASM customer")
        w("cannot call them at all. The Rust tests pass, the facade reaches them, and")
        w("the register would still say `shipped` -- which is why this list is")
        w("separate from the state column rather than folded into it.")
        w()
        for f in rust_only:
            w(f"- {f}")
        w()
    if unreachable:
        w("### Published crates a facade user cannot reach")
        w()
        w("Each is either a deliberate split (an engine internal, a vendored codec) or")
        w("a capability we imply we ship and do not wire up. The register cannot tell")
        w("those apart; a human has to say which is which, and then wire or document")
        w("it.")
        w()
        w("`" + "` · `".join(unreachable) + "`")
        w()

    w("## Facade surface")
    w()
    if stubs:
        w("### Methods that exist and fail when called")
        w()
        w("The dangerous shape: the type system promises them, the runtime refuses, and")
        w("a caller cannot discover the gap without running it.")
        w()
        for n in stubs:
            w(f"- `{n}()`")
        w()
    w("### All public methods")
    w()
    w("| method | works |")
    w("|---|---|")
    for n in sorted(methods):
        w(f"| `{n}` | {'**stub**' if methods[n] else 'yes'} |")
    w()

    w("## Crates")
    w()
    w("| crate | version | reachable from facade | test files | unit tests | run by |")
    w("|---|---|---|---|---|---|")
    for c in crates:
        has = c["tests"] or c["unit"]
        job = f"`{c['job']}`" if c["job"] else ("**nothing**" if has else "**no tests**")
        w(f"| `{c['name']}` | {c['version']} | {'yes' if c['reachable'] else '**no**'} "
          f"| {c['tests'] or '—'} | {c['unit'] or '—'} | {job} |")
    w()

    if a["empty_features"]:
        w("## Feature flags that enable nothing")
        w()
        w("Turning one of these on is a no-op, which reads as consent. Some are harmless")
        w("(the dependency is unconditional anyway) — check `cargo tree` before assuming")
        w("either way.")
        w()
        w("`" + "` · `".join(a["empty_features"]) + "`")
        w()

    w("## Keeping it honest")
    w()
    w("- Add a capability to `PROMISES` in `scripts/ci/feature_promises.py` in the same")
    w("  change that adds it to the website or the editor. That list is what this")
    w("  register walks; a promise made outside it is invisible here.")
    w("- If `sanity:capability-register` is red, run the script and commit the result")
    w("  in the same change. Read the diff first — it is the news.")
    w("- The register proves a test exists and runs. It cannot prove the test asserts")
    w("  anything. For that, break the function on purpose and watch the test fail;")
    w("  three tests in this repo passed against broken code before anyone did.")
    w()
    return "\n".join(L) + "\n"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    try:
        current = render(analyse())
    except Exception as e:  # noqa: BLE001
        print(f"[capability_register] FATAL: {e}", file=sys.stderr)
        sys.exit(2)

    if not args.check:
        OUT.parent.mkdir(parents=True, exist_ok=True)
        OUT.write_text(current)
        print(f"[capability_register] written: {OUT.relative_to(REPO)}")
        sys.exit(0)

    if not OUT.exists():
        print(f"[capability_register] FAIL: {OUT.relative_to(REPO)} is missing; "
              "run without --check and commit it", file=sys.stderr)
        sys.exit(1)
    if OUT.read_text() != current:
        print("[capability_register] FAIL: the register no longer matches the code.")
        print("[capability_register] Something shipped, broke, moved, or lost its CI job.")
        print("[capability_register] Run: python3 scripts/ci/capability_register.py")
        print("[capability_register] and commit the result in the same change.")
        sys.exit(1)
    print("[capability_register] up to date")
    sys.exit(0)


if __name__ == "__main__":
    main()
