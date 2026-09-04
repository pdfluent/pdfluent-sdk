#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""What the fetched-asset guard must catch, and what it must ignore.

The case that carries the design is the namespace one. There are 406 URLs in the
crate sources and almost all of them are XML namespaces -- identifiers, never
fetched. A guard that demanded a licence row for those would bury the two that
matter under four hundred that do not, and a guard nobody can read is a guard
nobody runs.
"""
from __future__ import annotations
import os, pathlib, subprocess, sys, tempfile

CI = pathlib.Path(__file__).resolve().parent
GUARD = CI / "every_fetched_asset_is_registered.py"

REGISTER = '''[[asset]]
url_prefix = "https://example-models.s3-accelerate.amazonaws.com"
what = "test weights"
used_by = "crates/x/src/lib.rs"
licence = "Apache-2.0"
licence_source = "stated on the page"
verified = "2026-09-04"
decision = "usable"
'''


def tree(tmp: pathlib.Path, register: str, source: str, n_files: int = 60) -> pathlib.Path:
    root = tmp / "repo"
    (root / "docs").mkdir(parents=True)
    src = root / "crates" / "x" / "src"
    src.mkdir(parents=True)
    (root / "docs" / "FETCHED_ASSETS.toml").write_text(register, encoding="utf-8")
    (src / "lib.rs").write_text(source, encoding="utf-8")
    # Above the floor, so the fixture tests behaviour and not the refusal.
    for i in range(n_files):
        (src / f"pad{i}.rs").write_text("pub fn v() {}\n", encoding="utf-8")
    return root


def run_guard(root: pathlib.Path):
    guard_dir = root / "scripts" / "ci"
    guard_dir.mkdir(parents=True, exist_ok=True)
    (guard_dir / GUARD.name).write_bytes(GUARD.read_bytes())
    return subprocess.run([sys.executable, str(guard_dir / GUARD.name)],
                          capture_output=True, text=True, env=dict(os.environ))


def case(label: str, ok: bool, why: str = "") -> bool:
    print(f"  {'ok  ' if ok else 'FAIL'}  {label}")
    if not ok and why:
        print(f"        {why}")
    return ok


def main() -> int:
    ok_all = True

    with tempfile.TemporaryDirectory() as d:
        root = tree(pathlib.Path(d), REGISTER,
                    'const M: &str = "https://example-models.s3-accelerate.amazonaws.com/a.rten";\n')
        u = run_guard(root)
        ok_all &= case("a registered fetch is green", u.returncode == 0,
                       (u.stdout + u.stderr)[:200])

    with tempfile.TemporaryDirectory() as d:
        root = tree(pathlib.Path(d), REGISTER,
                    'const M: &str = "https://other-models.s3.amazonaws.com/a.rten";\n')
        u = run_guard(root)
        ok_all &= case("an unregistered fetch is red",
                       u.returncode == 1 and "other-models" in u.stderr,
                       (u.stdout + u.stderr)[:250])

    # THE case: a namespace is an identifier, not a place bytes come from.
    with tempfile.TemporaryDirectory() as d:
        root = tree(pathlib.Path(d), REGISTER,
                    'const NS: &str = "http://www.xfa.org/schema/xfa-template/3.3/";\n')
        u = run_guard(root)
        ok_all &= case("an XML namespace is not a fetch", u.returncode == 0,
                       (u.stdout + u.stderr)[:250])

    with tempfile.TemporaryDirectory() as d:
        root = tree(pathlib.Path(d), REGISTER,
                    'const M: &str = "https://huggingface.co/who/what/resolve/main/a.onnx";\n')
        u = run_guard(root)
        ok_all &= case("a HuggingFace model file is a fetch",
                       u.returncode == 1 and "huggingface" in u.stderr,
                       (u.stdout + u.stderr)[:250])

    # A row with an empty field is not a row. Without this, "no licence stated"
    # could be written as no licence field at all, and the register would look
    # complete while saying nothing.
    with tempfile.TemporaryDirectory() as d:
        root = tree(pathlib.Path(d), REGISTER.replace('licence = "Apache-2.0"', 'licence = ""'),
                    'const M: &str = "https://example-models.s3-accelerate.amazonaws.com/a.rten";\n')
        u = run_guard(root)
        ok_all &= case("a row with an empty licence is red",
                       u.returncode == 1 and "licence" in u.stderr,
                       (u.stdout + u.stderr)[:250])

    # The floor. Without it a broken glob reports OK over nothing examined.
    with tempfile.TemporaryDirectory() as d:
        root = tree(pathlib.Path(d), REGISTER, "pub fn a() {}\n", n_files=2)
        u = run_guard(root)
        ok_all &= case("too few files examined is SKIPPED, not green",
                       u.returncode == 1 and "SKIPPED (not a pass)" in u.stderr,
                       (u.stdout + u.stderr)[:250])

    with tempfile.TemporaryDirectory() as d:
        root = tree(pathlib.Path(d), REGISTER, "pub fn a() {}\n")
        (root / "docs" / "FETCHED_ASSETS.toml").unlink()
        u = run_guard(root)
        ok_all &= case("a missing register is SKIPPED, not green",
                       u.returncode != 0 and "SKIPPED (not a pass)" in (u.stdout + u.stderr),
                       (u.stdout + u.stderr)[:250])

    print("test_every_fetched_asset_is_registered: " + ("OK" if ok_all else "FAILED"))
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
