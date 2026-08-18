#!/usr/bin/env python3
"""Generate the capability inventory from the code, and fail if it has drifted.

WHY THIS EXISTS

Three documents describing what this codebase can do were all four months stale
at once: XFA_KNOWN_LIMITATIONS.md, the website's credibility inventory, and my
own reading of the facade. Each read as fact. Acting on them produced wrong
claims — "PDF/UA is a one-to-two-week build" when pdf-compliance already carried
29,000 lines of it, "there is no text-replacement API" when the whole TextEditor
had shipped.

The pattern is not carelessness, it is that **a hand-written map of a moving
codebase is stale the day after it is written**, and nothing tells you. Writing a
better one does not fix that; it just resets the clock.

So this one is generated from the source, and CI fails when the committed copy no
longer matches what the code says. A map that cannot silently rot is worth more
than a more detailed map that can.

Usage:
    capability_inventory.py            # regenerate docs/CAPABILITY_INVENTORY.md
    capability_inventory.py --check    # fail if the committed copy is stale

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
OUT = REPO / "docs" / "CAPABILITY_INVENTORY.md"

# Bodies that mean "this method exists but does nothing".
STUB_MARKERS = ("MissingDependency", "UnsupportedOnWasm", "todo!", "unimplemented!")


def run(cmd: list[str]) -> str:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True).stdout


def workspace_crates() -> list[dict]:
    meta = json.loads(run(["cargo", "metadata", "--format-version", "1", "--no-deps"]) or "{}")
    return sorted(meta.get("packages", []), key=lambda p: p["name"])


def facade_deps() -> set[str]:
    """Crates the facade actually depends on — the authoritative reachability test."""
    tree = run(["cargo", "tree", "-p", "pdfluent", "--depth", "1", "--edges", "normal"])
    return {m.group(1) for m in re.finditer(r"^[│├└─\s]*([a-z0-9_-]+) v", tree, re.M)}


def facade_methods() -> list[tuple[str, bool]]:
    """(name, is_stub) for every public method on Document."""
    src = (REPO / "crates" / "pdfluent" / "src" / "document.rs").read_text(errors="replace")
    out: dict[str, bool] = {}
    for m in re.finditer(r"pub fn (\w+)", src):
        name = m.group(1)
        body = src[m.end() : m.end() + 900]
        stub = any(k in body.split("\n    pub fn ")[0] for k in STUB_MARKERS)
        # A method with both a real and a wasm-stub arm counts as real.
        out[name] = out.get(name, True) and stub
    return sorted(out.items())


def empty_features() -> list[str]:
    t = (REPO / "crates" / "pdfluent" / "Cargo.toml").read_text(errors="replace")
    m = re.search(r"\[features\](.*?)(\n\[|\Z)", t, re.S)
    return [
        l.split("=")[0].strip()
        for l in (m.group(1) if m else "").splitlines()
        if "=" in l and l.split("=", 1)[1].strip() in ("[]", "[ ]")
    ]


def render() -> str:
    crates = workspace_crates()
    deps = facade_deps()
    methods = facade_methods()
    stubs = [n for n, s in methods if s]
    empties = empty_features()

    publish = [c for c in crates if c.get("publish") != []]
    reachable = sorted(c["name"] for c in publish if c["name"] in deps)
    unreachable = sorted(
        c["name"] for c in publish
        if c["name"] not in deps and c["name"] not in ("pdfluent",)
        and not c["name"].startswith(("xfa-", "pdfluent-cff", "pdfluent-jbig2",
                                      "pdfluent-ccitt", "pdfluent-jpeg2000"))
    )

    L = []
    L.append("# Capability inventory")
    L.append("")
    L.append("**Generated** by `scripts/ci/capability_inventory.py` — do not edit by hand.")
    L.append("CI regenerates it and fails if this file has drifted from the code, which is")
    L.append("the whole point: three hand-written maps of this codebase were four months")
    L.append("stale at the same time, and each one still read as fact.")
    L.append("")
    L.append("## Facade surface")
    L.append("")
    L.append(f"`pdfluent::Document` exposes **{len(methods)} public methods**, "
             f"of which **{len(stubs)} do nothing at runtime**.")
    L.append("")
    if stubs:
        L.append("### Methods that exist and fail when called")
        L.append("")
        L.append("These are the dangerous ones: the type system promises them and the")
        L.append("runtime refuses, so a caller cannot discover the gap without running it.")
        L.append("")
        for n in stubs:
            L.append(f"- `{n}()`")
        L.append("")
    if empties:
        L.append("### Feature flags that enable nothing")
        L.append("")
        L.append("Enabling one of these is a no-op, which reads as consent. Some are")
        L.append("harmless (the dependency is unconditional anyway); check `cargo tree`")
        L.append("before assuming either way.")
        L.append("")
        L.append("`" + "` · `".join(empties) + "`")
        L.append("")
    L.append("## Crates reachable from the facade")
    L.append("")
    L.append("A customer using the `pdfluent` crate gets these:")
    L.append("")
    for c in reachable:
        L.append(f"- `{c}`")
    L.append("")
    L.append("## Published crates NOT reachable from the facade")
    L.append("")
    L.append("These exist and are published, but a customer using `pdfluent` cannot call")
    L.append("them without adding the crate themselves. Every entry here is either a")
    L.append("deliberate split or an advertised capability that is not actually delivered.")
    L.append("")
    for c in unreachable:
        L.append(f"- `{c}`")
    L.append("")
    L.append("## All public methods on the facade")
    L.append("")
    L.append("| method | works |")
    L.append("|---|---|")
    for n, s in methods:
        L.append(f"| `{n}` | {'**stub**' if s else 'yes'} |")
    L.append("")
    return "\n".join(L) + "\n"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    try:
        current = render()
    except Exception as e:  # noqa: BLE001
        print(f"[capability_inventory] FATAL: {e}", file=sys.stderr)
        sys.exit(2)

    if not args.check:
        OUT.parent.mkdir(parents=True, exist_ok=True)
        OUT.write_text(current)
        print(f"[capability_inventory] written: {OUT.relative_to(REPO)}")
        sys.exit(0)

    if not OUT.exists():
        print(f"[capability_inventory] FAIL: {OUT.relative_to(REPO)} does not exist; "
              "run the script without --check and commit it", file=sys.stderr)
        sys.exit(1)

    if OUT.read_text() != current:
        print("[capability_inventory] FAIL: the committed inventory no longer matches the code.")
        print("[capability_inventory] Something was added, removed, or stopped being a stub.")
        print("[capability_inventory] Run: python3 scripts/ci/capability_inventory.py")
        print("[capability_inventory] and commit the result in the same change.")
        sys.exit(1)

    print("[capability_inventory] up to date")
    sys.exit(0)


if __name__ == "__main__":
    main()
