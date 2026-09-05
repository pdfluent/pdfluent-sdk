#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""The public-API export says what it read, and refuses to say nothing.

`export_public_api.py` produces the list pdfluent.com checks its documentation
against. Every way for that list to be wrong is quiet: a `pub use` rename read
backwards publishes a name the crate does not have, a lowercase re-export
counted as a module invents a path, and an extraction that matches nothing
produces an export that approves the entire site. None of those show up as an
error; they show up as a green tick over a wrong answer, months later, in a
reader's first `cargo build`.

So the judgements live in functions -- `reexported`, `crate_root_surface`, the
two patterns and the floors -- and this file drives them on synthetic input, in
milliseconds, without a compiler.

Mutation-tested, which is the only reason to believe it. Each check below was
broken in `export_public_api.py` one at a time -- the rename resolved to the
wrong side, the brace test dropped, a floor removed -- and each time this file
went red.

Exit codes:
    0  every case holds
    1  a case failed
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "ci"))

import export_public_api as export  # noqa: E402


def case_rename_publishes_the_new_name() -> tuple[bool, str]:
    names, _ = export.reexported("pub use crate::a::{B, C as D};")
    return ({"B", "D"} == names, str(sorted(names)))


def case_a_braced_lowercase_item_is_not_a_module() -> tuple[bool, str]:
    """`pub use crate::license::{license_info, set_license_key};` publishes two
    functions. Calling them modules would have the site accept
    `pdfluent::license_info::` as a path to something."""
    names, modules = export.reexported(
        "pub use crate::license::{license_info, set_license_key};"
    )
    return (modules == set() and names == {"license_info", "set_license_key"},
            f"modules={sorted(modules)} names={sorted(names)}")


def case_an_unbraced_lowercase_path_is_a_module() -> tuple[bool, str]:
    _, modules = export.reexported("pub use pdf_manip::text_edit;")
    return (modules == {"text_edit"}, str(sorted(modules)))


def case_a_type_reexport_is_not_a_module() -> tuple[bool, str]:
    _, modules = export.reexported("pub use pdf_engine::ProcessingLimits;")
    return (modules == set(), str(sorted(modules)))


def case_every_pub_fn_shape_is_seen() -> tuple[bool, str]:
    src = """
        pub fn plain() {}
        pub const fn constant() {}
        pub async fn asynchronous() {}
        pub unsafe fn dangerous() {}
        pub extern "C" fn abi() {}
        pub(crate) fn restricted() {}
        fn private() {}
    """
    found = {m.group(1) for m in export.PUB_FN.finditer(src)}
    want = {"plain", "constant", "asynchronous", "dangerous", "abi", "restricted"}
    return (found == want, str(sorted(found)))


def case_private_items_stay_out() -> tuple[bool, str]:
    found = {m.group(1) for m in export.PUB_TYPE.finditer(
        "pub struct Kept; struct Dropped; pub enum Also {} pub type Alias = u8;")}
    return (found == {"Kept", "Also", "Alias"}, str(sorted(found)))


def case_the_floors_are_real_numbers() -> tuple[bool, str]:
    """A floor of zero is not a floor. Each one must be high enough that a
    broken extraction cannot slip under it."""
    low = [k for k, v in export.FLOORS.items() if v < 10]
    return (not low, f"too low: {low}")


def case_the_committed_export_is_not_stale() -> tuple[bool, str]:
    """The same question `--check` asks in CI, asked here so that a failure
    names this file rather than a build step three jobs later."""
    r = subprocess.run(
        [sys.executable, str(REPO / "scripts" / "ci" / "export_public_api.py"), "--check"],
        capture_output=True, text=True,
    )
    return (r.returncode == 0, (r.stderr or r.stdout).strip()[:300])


def case_the_export_carries_methods() -> tuple[bool, str]:
    """The reason this export was rebuilt at all (#245). A copy without
    `workspace_methods` is the old one, and the site's method check would refuse
    to run against it -- but only if this stays true."""
    out = REPO / "docs" / "PUBLIC_API.json"
    if not out.is_file():
        return (False, "docs/PUBLIC_API.json is missing")
    d = json.loads(out.read_text(encoding="utf-8"))
    ok = (
        len(d.get("workspace_methods", [])) >= export.FLOORS["workspace_methods"]
        and isinstance(d.get("facade_methods"), dict)
        and any(works is False for works in d["facade_methods"].values())
    )
    return (ok, f"{len(d.get('workspace_methods', []))} methods, "
                f"{len(d.get('facade_methods', {}))} facade entries")


CASES = [
    case_rename_publishes_the_new_name,
    case_a_braced_lowercase_item_is_not_a_module,
    case_an_unbraced_lowercase_path_is_a_module,
    case_a_type_reexport_is_not_a_module,
    case_every_pub_fn_shape_is_seen,
    case_private_items_stay_out,
    case_the_floors_are_real_numbers,
    case_the_committed_export_is_not_stale,
    case_the_export_carries_methods,
]


def main() -> int:
    failed = []
    for case in CASES:
        ok, detail = case()
        print(f"  {'ok  ' if ok else 'FAIL'}  {case.__name__}")
        if not ok:
            failed.append((case.__name__, detail))
    if failed:
        print("\n[public-api-export] the export no longer says what it read:\n",
              file=sys.stderr)
        for name, detail in failed:
            print(f"  - {name}\n      got: {detail}", file=sys.stderr)
        return 1
    print(f"[public-api-export] {len(CASES)} cases, all good")
    return 0


if __name__ == "__main__":
    sys.exit(main())
