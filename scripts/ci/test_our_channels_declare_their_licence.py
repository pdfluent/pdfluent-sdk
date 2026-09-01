#!/usr/bin/env python3
"""Two-way proof for our_channels_declare_their_licence.py (#300, #304).

Fixture-based on purpose. Asserting against the real tree would encode today's
state -- and today the real tree FAILS, because crates/pdf-node/package.json
still says "SEE LICENSE IN LICENSE". A test that pinned that would have to be
rewritten by whoever fixes it, which is how a suite starts arguing for the bug.
"""
from __future__ import annotations
import json, pathlib, shutil, subprocess, sys, tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = pathlib.Path("scripts/ci/our_channels_declare_their_licence.py")
CANON = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"

fails: list[str] = []
ran = 0


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f"   [{detail}]" if not ok and detail else ""))
    if not ok:
        fails.append(what)


def build(npm=CANON, cargo=CANON, csproj=CANON, py=CANON, policy=None):
    """A tree with four channel manifests and a policy naming them."""
    td = tempfile.mkdtemp()
    root = pathlib.Path(td) / "repo"
    (root / "scripts" / "ci").mkdir(parents=True)
    (root / "docs").mkdir(parents=True)
    shutil.copy(REPO / GUARD, root / GUARD)

    (root / "crates" / "pdf-node").mkdir(parents=True)
    (root / "crates" / "pdf-node" / "package.json").write_text(
        json.dumps({"name": "pdfluent", **({"license": npm} if npm else {})}))

    (root / "crates" / "xfa-wasm").mkdir(parents=True)
    (root / "crates" / "xfa-wasm" / "Cargo.toml").write_text(
        "[package]\nname = \"x\"\n" + (f'license = "{cargo}"\n' if cargo else ""))

    d = root / "bindings" / "dotnet" / "src" / "PDFluent"
    d.mkdir(parents=True)
    expr = f"  <PackageLicenseExpression>{csproj}</PackageLicenseExpression>\n" if csproj else ""
    (d / "PDFluent.csproj").write_text(
        f"<Project>\n <PropertyGroup>\n{expr}  <PackageLicenseFile>LICENSE</PackageLicenseFile>\n"
        " </PropertyGroup>\n</Project>\n")

    (root / "crates" / "pdf-python").mkdir(parents=True)
    (root / "crates" / "pdf-python" / "pyproject.toml").write_text(
        "[project]\nname = \"pdfluent\"\n" + (f'license = {{ text = "{py}" }}\n' if py else ""))

    if policy is None:
        policy = "\n".join([
            "[own_packages]",
            f'"crates/pdf-node/package.json" = "{CANON}"',
            f'"crates/xfa-wasm/Cargo.toml" = "{CANON}"',
            f'"bindings/dotnet/src/PDFluent/PDFluent.csproj" = "{CANON}"',
            f'"crates/pdf-python/pyproject.toml" = "{CANON}"',
        ])
    (root / "docs" / "LICENSE_POLICY.toml").write_text(policy + "\n")
    return root


def run(root: pathlib.Path) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, str(root / GUARD)],
                          capture_output=True, text=True)


print("our channels declare their licence — two-way")

r = run(build())
expect("all four channels declaring the canonical expression passes",
       r.returncode == 0, f"exit={r.returncode} {r.stderr[-200:]}")

# The exact three gaps measured on the #257 flip branch.
r = run(build(npm="SEE LICENSE IN LICENSE"))
expect("npm's file pointer FAILS", r.returncode == 1, f"exit={r.returncode}")
expect("  and says the comparison is skipped, not that it passed",
       "skips the comparison" in r.stderr, r.stderr[-200:])

r = run(build(csproj=None))
expect("a csproj with no PackageLicenseExpression FAILS", r.returncode == 1,
       f"exit={r.returncode}")
expect("  and names the file", "PDFluent.csproj" in r.stderr)

r = run(build(py=None))
expect("a pyproject with no project.license FAILS", r.returncode == 1,
       f"exit={r.returncode}")

r = run(build(cargo=None))
expect("a Cargo.toml with no license FAILS", r.returncode == 1, f"exit={r.returncode}")

r = run(build(npm="MIT"))
expect("a channel declaring something else FAILS", r.returncode == 1,
       f"exit={r.returncode}")
expect("  and says one of the two is out of date", "out of date" in r.stderr)

# A dual expression is the point, not a violation: the canonical value has to be
# one of its operands, not the whole string.
r = run(build(npm=f"{CANON} AND MIT"))
expect("a wider expression containing the canonical one passes",
       r.returncode == 0, f"exit={r.returncode} {r.stderr[-160:]}")

# One canonical string across four registers, or the registers can only be
# checked against each other.
r = run(build(policy="\n".join([
    "[own_packages]",
    f'"crates/pdf-node/package.json" = "{CANON}"',
    '"crates/xfa-wasm/Cargo.toml" = "AGPL-3.0-or-later"',
])))
expect("two different expressions in own_packages FAILS", r.returncode == 1,
       f"exit={r.returncode}")
expect("  and explains why one constant matters", "one constant" in r.stderr)

r = run(build(policy="[licenses]\nallowed = []"))
expect("a policy naming no manifest is FATAL, not a pass", r.returncode == 2,
       f"exit={r.returncode}")

root = build()
(root / "docs" / "LICENSE_POLICY.toml").unlink()
r = run(root)
expect("a missing policy is FATAL, not a pass", r.returncode == 2, f"exit={r.returncode}")
expect("  and does not crash", "Traceback" not in r.stderr)

MINIMUM_CASES = 15  # FLOOR
print(f"\n  {ran} assertion(s) ran, {len(fails)} failure(s)")
for f in fails:
    print(f"    - {f}")
if ran < MINIMUM_CASES:
    print(f"  FATAL: {ran} assertions ran, floor is {MINIMUM_CASES}.", file=sys.stderr)
    raise SystemExit(2)
raise SystemExit(1 if fails else 0)
