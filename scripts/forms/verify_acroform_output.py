#!/usr/bin/env python3
"""External-compatibility verifier for SDK-filled AcroForm output.

Opens each `<category>.filled.pdf` produced by the `fill_acroform_corpus`
example with **pikepdf** (a third-party qpdf binding — independent of our own
parser) and asserts the structural invariants of the support contract:
`/V`, `/AS`, `/AP`, `/I`, and `/NeedAppearances`. Also runs `qpdf --check`
for structural soundness when qpdf is available.

Usage:
    python3 scripts/forms/verify_acroform_output.py <filled_dir>

Exit code 0 when every category passes; 1 otherwise (with a per-category
report on stdout).
"""
import json
import subprocess
import sys
from pathlib import Path

import pikepdf


def _find(pdf, fqn):
    """Find a field dict by fully-qualified name (dot-joined /T chain)."""
    def walk(node, prefix):
        t = node.get("/T")
        partial = str(t) if t is not None else ""
        name = partial if not prefix else (prefix if not partial else f"{prefix}.{partial}")
        if name == fqn and "/FT" in node:
            return node
        kids = node.get("/Kids")
        if kids is not None:
            for kid in kids:
                found = walk(kid, name)
                if found is not None:
                    return found
        return None

    acro = pdf.Root.get("/AcroForm")
    if acro is None:
        return None
    for f in acro.get("/Fields", []):
        found = walk(f, "")
        if found is not None:
            return found
    return None


def _v_str(field):
    v = field.get("/V")
    if v is None:
        return None
    if isinstance(v, pikepdf.Name):
        return str(v)[1:]  # strip leading /
    if isinstance(v, pikepdf.Array):
        return [str(x) for x in v]
    s = str(v)
    return s


def _has_ap(field):
    ap = field.get("/AP")
    return ap is not None and "/N" in ap


def check_category(path: Path, spec: dict) -> list[str]:
    """Return a list of failure strings (empty = pass)."""
    failures = []
    try:
        pdf = pikepdf.open(str(path))
    except Exception as e:  # noqa: BLE001
        return [f"pikepdf.open failed: {e}"]

    acro = pdf.Root.get("/AcroForm")
    if acro is None:
        return ["no /AcroForm in saved output"]

    for fqn, want in spec.items():
        field = _find(pdf, fqn)
        if field is None:
            failures.append(f"{fqn}: field not found")
            continue

        if "v" in want:
            got = _v_str(field)
            if got != want["v"]:
                failures.append(f"{fqn}: /V text {got!r} != {want['v']!r}")
        if "v_name" in want:
            got = _v_str(field)
            if got != want["v_name"]:
                failures.append(f"{fqn}: /V name {got!r} != {want['v_name']!r}")
        if "v_array" in want:
            got = _v_str(field)
            if got != want["v_array"]:
                failures.append(f"{fqn}: /V array {got!r} != {want['v_array']!r}")
        if "i" in want:
            i = field.get("/I")
            got = [int(x) for x in i] if i is not None else None
            if got != want["i"]:
                failures.append(f"{fqn}: /I {got!r} != {want['i']!r}")
        if "as" in want:
            asv = field.get("/AS")
            got = str(asv)[1:] if asv is not None else None
            # /AS lives on the widget; for single-widget fields it is on the
            # field dict itself.
            if got != want["as"]:
                # Try kid widgets.
                kids = field.get("/Kids", [])
                kid_states = [str(k.get("/AS"))[1:] for k in kids if k.get("/AS") is not None]
                if want["as"] not in ([got] + kid_states):
                    failures.append(f"{fqn}: /AS {got!r}/{kid_states} != {want['as']!r}")
        if want.get("ap"):
            if not _has_ap(field):
                failures.append(f"{fqn}: expected /AP /N, missing")
        if want.get("utf16"):
            raw = field.get("/V")
            b = bytes(raw) if raw is not None else b""
            if not b.startswith(b"\xfe\xff"):
                failures.append(f"{fqn}: expected UTF-16BE BOM in /V")
        if want.get("needs_appearances"):
            na = acro.get("/NeedAppearances")
            if not (na is not None and bool(na)):
                failures.append(f"{fqn}: expected /NeedAppearances true")
    return failures


def qpdf_check(path: Path) -> str | None:
    """Run `qpdf --check`; return None on pass, else the error text."""
    try:
        r = subprocess.run(
            ["qpdf", "--check", str(path)],
            capture_output=True,
            text=True,
            timeout=30,
        )
    except FileNotFoundError:
        return None  # qpdf not installed — skip
    except subprocess.TimeoutExpired:
        return "qpdf --check timed out"
    # qpdf exit 0 = clean, 3 = warnings (acceptable for our minimal fixtures).
    if r.returncode not in (0, 3):
        return (r.stdout + r.stderr).strip()[:400]
    return None


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    filled_dir = Path(sys.argv[1])
    manifest = json.loads((filled_dir / "manifest.json").read_text())

    total = 0
    passed = 0
    for category, spec in manifest.items():
        total += 1
        path = filled_dir / f"{category}.filled.pdf"
        if not path.exists():
            print(f"FAIL  {category}: {path.name} missing")
            continue
        # Drop non-structural manifest hints the verifier can't assert.
        spec = {
            k: {kk: vv for kk, vv in v.items() if kk not in ("fillable", "writable")}
            for k, v in spec.items()
        }
        spec = {k: v for k, v in spec.items() if v}
        failures = check_category(path, spec)
        qpdf_err = qpdf_check(path)
        if qpdf_err:
            failures.append(f"qpdf: {qpdf_err}")
        if failures:
            print(f"FAIL  {category}")
            for f in failures:
                print(f"        - {f}")
        else:
            passed += 1
            print(f"PASS  {category}")

    print(f"\n{passed}/{total} categories passed external pikepdf+qpdf verification")
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
