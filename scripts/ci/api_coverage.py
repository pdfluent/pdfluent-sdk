#!/usr/bin/env python3
"""Is every method we ship actually exercised by a test?

WHY THIS EXISTS

`convertToPdfa` was broken in the WebAssembly build for three months. Every
native suite stayed green, because the fault was in the binding rather than the
logic. When the WASM smoke suite was finally run it turned out to cover 16 of
25 exported methods — and `convertToPdfa` was among the nine it missed.

That was one binding. We ship six. This answers the same question for all of
them at once: for each thing a customer can call, does any test call it?

WHAT IT DOES NOT CLAIM

A method being *called* by a test is a low bar, deliberately. It does not mean
the behaviour is correct, or that edge cases are covered — the native suites do
that work. It means the binding survives the crossing, which is the failure this
was built to catch and the one no amount of logic testing finds.

So read a 100% here as "nothing is completely untested", not as "everything is
well tested". The two are different claims and only the first is being made.

A NOTE ON TRUSTING THIS NUMBER

Six separate matching bugs were found and fixed while writing it, and every one
of them *understated* coverage: C tests were not being read at all, JNI symbols
never match the Java method names a test calls, napi renames snake_case to
camelCase, Python getters are read as attributes rather than called. The figure
moved 33% -> 45% -> 53% -> 61% -> 75% as each was fixed.

So treat the number as a floor, and if a binding reads suspiciously low, suspect
the matcher before concluding the tests are missing. Reporting the first figure
would have raised an alarm that was mostly wrong.

Exit codes:
    0  every exported symbol is exercised
    1  gaps found
    2  could not run (no bindings located)
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent


@dataclass
class Binding:
    name: str
    src_globs: list[str]
    test_globs: list[str]
    # (regex, group holding the exported name)
    export_patterns: list[tuple[str, int]]
    impl_attrs: list[str] = field(default_factory=list)
    exported: dict[str, str] = field(default_factory=dict)   # export name -> rust fn
    covered: set[str] = field(default_factory=set)


BINDINGS = [
    Binding(
        name="WASM (@pdfluent/sdk-wasm)",
        src_globs=["crates/xfa-wasm/src/*.rs"],
        test_globs=["crates/xfa-wasm/tests/*.rs"],
        export_patterns=[(r'js_name\s*=\s*"(\w+)"\s*\)\]\s*pub fn (\w+)', 2)],
        impl_attrs=["#[wasm_bindgen]"],
    ),
    Binding(
        name="Python (pdfluent)",
        src_globs=["crates/pdf-python/src/*.rs"],
        test_globs=["crates/pdf-python/tests/*.rs", "crates/pdf-python/tests/*.py",
                    "crates/pdf-python/python/**/*.py"],
        export_patterns=[(r'#\[pyo3[^\]]*\]\s*(?:#\[[^\]]*\]\s*)*fn (\w+)', 1)],
        impl_attrs=["#[pymethods]"],
    ),
    Binding(
        name="C ABI (voedt .NET/Java/Node)",
        src_globs=["crates/pdf-capi/src/*.rs"],
        test_globs=["crates/pdf-capi/tests/*.rs", "crates/pdf-capi/tests/*.c",
                    "crates/pdf-capi/tests/*.h"],
        export_patterns=[(r'#\[no_mangle\]\s*pub (?:unsafe )?extern "C" fn (\w+)', 1)],
    ),
    Binding(
        name="Java (JNI)",
        src_globs=["crates/pdf-java/src/*.rs"],
        test_globs=["crates/pdf-java/tests/*.rs",
                    "crates/pdf-java/src/test/java/**/*.java",
                    "bindings/java/src/test/java/**/*.java"],
        export_patterns=[(r'extern "system" fn (\w+)', 1)],
    ),
    Binding(
        name="Node (napi)",
        src_globs=["crates/pdf-node/src/*.rs"],
        test_globs=["crates/pdf-node/tests/*.rs", "crates/pdf-node/tests/*.js",
                    "crates/pdf-node/tests/*.mjs", "crates/pdf-node/__test__/*.js"],
        export_patterns=[(r'#\[napi[^\]]*\]\s*(?:#\[[^\]]*\]\s*)*pub fn (\w+)', 1)],
        impl_attrs=["#[napi]"],
    ),
]


def impl_blocks(src: str, attrs: list[str]) -> list[str]:
    """Bodies of impl blocks carrying one of `attrs`, brace-matched."""
    out = []
    for attr in attrs:
        for m in re.finditer(re.escape(attr), src):
            start = src.find("{", m.end())
            if start == -1:
                continue
            depth, i = 0, start
            while i < len(src):
                if src[i] == "{":
                    depth += 1
                elif src[i] == "}":
                    depth -= 1
                    if depth == 0:
                        out.append(src[start : i + 1])
                        break
                i += 1
    return out


def read_all(globs: list[str]) -> str:
    out = []
    for g in globs:
        for f in REPO.glob(g):
            if "node_modules" in str(f) or "/target/" in str(f):
                continue
            out.append(f.read_text(errors="replace"))
    return "\n".join(out)


def js_to_snake(name: str) -> str:
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def snake_to_camel(name: str) -> str:
    """napi-rs and wasm-bindgen rename snake_case exports to camelCase, so a
    test calls `extractText` for a Rust `extract_text`. Comparing only the Rust
    spelling reported those as untested — which is how the Node figure first
    came out at 20% instead of the truth."""
    head, *rest = name.split("_")
    return head + "".join(p[:1].upper() + p[1:] for p in rest)


def main() -> None:
    any_found = False
    total_exported = total_covered = 0
    report: list[str] = []

    for b in BINDINGS:
        src = read_all(b.src_globs)
        if not src.strip():
            report.append(f"  {b.name}: geen bronbestanden gevonden — overgeslagen")
            continue
        tests = read_all(b.test_globs)
        any_found = True

        # Methods inside an attributed impl block are exported too, even without
        # a per-method attribute. Counting only annotated methods undercounted
        # Python by more than half on the first version of this script, which
        # would have flattered the result — the direction that matters.
        for block in impl_blocks(src, b.impl_attrs):
            for m in re.finditer(r"\n\s*(?:pub )?fn (\w+)", block):
                fn = m.group(1)
                if fn.startswith("_"):
                    continue
                js = re.search(rf'js_name\s*=\s*"(\w+)"[^)]*\)\]\s*(?:pub )?fn {fn}\b', src)
                b.exported.setdefault(fn, js.group(1) if js else fn)

        for pattern, group in b.export_patterns:
            for m in re.finditer(pattern, src, re.S):
                rust_fn = m.group(group)
                b.exported[rust_fn] = m.group(1) if group == 2 else rust_fn

        for rust_fn, js_name in b.exported.items():
            # A test may call it by its Rust name or by the exported name.
            names = {rust_fn, js_name, js_to_snake(js_name), snake_to_camel(rust_fn)}
            # JNI symbols are Java_<pkg>_<Class>_<method>; a Java test calls the
            # method, never the symbol. Match on the tail, or Java reads 0%
            # while its tests are in fact exercising the natives.
            if rust_fn.startswith("Java_"):
                # Take the LAST underscore-separated segment: `\w` matches the
                # underscore too, so a lazy middle grabs the whole package path
                # and the method name never appears.
                tail = rust_fn.rsplit("_", 1)[-1]
                for cand in (tail, re.sub(r"^native", "", tail)):
                    if cand:
                        names.add(cand)
                        names.add(cand[0].lower() + cand[1:])
            # Property getters are read as attributes, not called, so requiring
            # a following "(" reported every getter as untested. Python's
            # #[getter] methods are matched on attribute access instead.
            is_getter = re.search(
                rf"#\[getter\][^}}]{{0,200}}?fn {re.escape(rust_fn)}\b", src, re.S
            )
            hit = any(re.search(rf"\b{re.escape(n)}\s*\(", tests) for n in names)
            if not hit and is_getter:
                hit = any(re.search(rf"\.{re.escape(n)}\b", tests) for n in names)
            if hit:
                b.covered.add(rust_fn)

        n, c = len(b.exported), len(b.covered)
        total_exported += n
        total_covered += c
        pct = (100 * c // n) if n else 0
        mark = "OK  " if c == n and n else "GAP "
        report.append(f"  {mark}{b.name:32} {c}/{n} ({pct}%)"
                      + ("" if tests.strip() else "   [GEEN testbestanden gevonden]"))
        for missing in sorted(set(b.exported) - b.covered):
            report.append(f"        ongedekt: {b.exported[missing]}")

    if not any_found:
        print("[api_coverage] FATAL: geen bindings gevonden — de analyse keek naar niets",
              file=sys.stderr)
        sys.exit(2)

    print("=" * 70)
    print("[api_coverage] Wordt elke methode die we uitleveren door een test aangeroepen?")
    print("=" * 70)
    print("\n".join(report))
    print("-" * 70)
    pct = (100 * total_covered // total_exported) if total_exported else 0
    print(f"  totaal: {total_covered}/{total_exported} ({pct}%)")
    print()
    print("  Let op: 'aangeroepen door een test' is een lage lat en met opzet zo.")
    print("  Het zegt dat de binding de oversteek overleeft, niet dat het gedrag klopt.")

    # A floor that only moves up.
    #
    # This job was allow_failure because 100% is not today's reality, and a job
    # that always fails is a job everybody ignores. But "allowed to fail" also
    # means allowed to slide: coverage could drop from 198 to 150 and the output
    # would look exactly the same. A ratchet fixes both -- the job passes at
    # today's number and fails the moment it drops, so it can be a hard gate
    # while the remaining gaps are closed one at a time.
    #
    # Raise FLOOR when you close gaps. Never lower it: lowering it is the thing
    # this exists to prevent, and if a binding legitimately loses a method the
    # right change is removing it from the export list.
    FLOOR = 198

    if total_covered < FLOOR:
        print()
        print(f"  FAIL: coverage dropped to {total_covered}, below the floor of {FLOOR}.")
        print("  Something that had a test no longer does. The list above names it.")
        print("  If a method was deliberately removed, lower FLOOR in this file in the")
        print("  same commit and say why -- deliberately, not as a reflex.")
        sys.exit(1)

    if total_covered > FLOOR:
        print()
        print(f"  Coverage is {total_covered}, above the floor of {FLOOR}.")
        print(f"  Raise FLOOR to {total_covered} in scripts/ci/api_coverage.py so the")
        print("  gain cannot be lost again silently.")

    if total_covered == total_exported:
        print()
        print("  Every exported method is called by a test. Retire the floor.")

    sys.exit(0)


if __name__ == "__main__":
    main()
