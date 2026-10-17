#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""source_literals_name_no_host_path.py over fixture crates, then the tree.

The case it exists for is the 1.1.0 CLI message that wrote an example drive
path inside a Windows-only branch: the fixture holds that exact shape and
must fail. Literals that never ship (#[cfg(test)], comments) must pass, and
so must the real published crates.
"""
from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
GUARD = HERE / "source_literals_name_no_host_path.py"

# The fixtures need real host roots, but this file ships in the published
# tree and must not hold them as bytes (public_snapshot_check.sh greps for
# them). Each fixture spells them @USERS@ / @HOME@ and _host() fills them in.
_USERS, _HOME = "/Use" "rs/", "/ho" "me/"


def _host(body: str) -> str:
    return body.replace("@USERS@", _USERS).replace("@HOME@", _HOME)

LEAK_1_1_0 = r'''
fn parse(url: &str, windows: bool) -> Result<String, String> {
    if windows {
        return Err(format!("{url:?} names no drive; on Windows write file:///C:/dir/"));
    }
    Ok(url.to_string())
}
'''
LEAK_ESCAPED = r'''
pub const WHERE: &str = "C:\\actions-runner\\work\\x.rs";
'''
LEAK_RAW = _host(r'''
pub const WHERE: &str = r#"see @USERS@someone/src/x.rs"#;
''')
LEAK_AFTER_TEST_MODULE = _host(r'''
#[cfg(test)]
mod tests {
    #[test]
    fn t() { assert_eq!(super::f(), "ok"); let _ = '{'; }
}
pub fn f() -> &'static str { "@HOME@runner/work/x.rs" }
''')
QUIET_TEST_ONLY = r'''
pub fn f() -> &'static str { "ok" }
#[cfg(test)]
mod tests {
    #[test]
    fn drive_paths_in_tests_never_ship() {
        let want = "C:/x/heron_ep03.onnx";
        let brace = '}';
        assert_ne!(super::f(), want, "{brace}");
    }
}
'''
QUIET_COMMENT_AND_URL = _host(r'''
// An example such as file:///C:/dir/ in a comment never ships.
/* Nor @USERS@someone/x in a block comment. */
pub const DOCS: &str = "https://example.invalid@USERS@someone/x/";
pub const HINT: &str = "names no drive letter; on Windows put one after file:///";
''')


def crate(root: Path, name: str, body: str) -> Path:
    d = root / name
    (d / "src").mkdir(parents=True)
    (d / "src" / "lib.rs").write_text(body, encoding="utf-8")
    return d


def run(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(GUARD), *args],
                          capture_output=True, text=True)


def main() -> int:
    failures: list[str] = []
    cases = 0
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        for name, body in [("leak110", LEAK_1_1_0), ("escaped", LEAK_ESCAPED),
                           ("raw", LEAK_RAW), ("aftertest", LEAK_AFTER_TEST_MODULE)]:
            cases += 1
            r = run(str(crate(root, name, body)))
            if r.returncode != 1 or "lib.rs" not in r.stdout:
                failures.append(f"{name}: a shipped host-path literal must fail "
                                f"(rc={r.returncode}): {r.stdout}{r.stderr}")
        cases += 1
        r = run(str(crate(root, "leakline", LEAK_1_1_0)))
        if "lib.rs:4:" not in r.stdout:
            failures.append(f"the hit must name its line (lib.rs:4): {r.stdout}")
        for name, body in [("testonly", QUIET_TEST_ONLY), ("comment", QUIET_COMMENT_AND_URL)]:
            cases += 1
            r = run(str(crate(root, name, body)))
            if r.returncode != 0:
                failures.append(f"{name}: literals that never ship must pass "
                                f"(rc={r.returncode}): {r.stdout}{r.stderr}")
        cases += 1
        empty = root / "empty"
        (empty / "src").mkdir(parents=True)
        r = run(str(empty))
        if r.returncode != 2:
            failures.append(f"a crate with no sources must not pass vacuously (rc={r.returncode})")
    cases += 1
    r = run()
    if r.returncode != 0 or "OK:" not in r.stdout:
        failures.append(f"the published crates must pass: rc={r.returncode}\n{r.stdout}{r.stderr}")
    for f in failures:
        print("FAIL:", f)
    print(f"{cases} case(s), {len(failures)} failure(s)")
    if failures:
        return 1
    print("OK: a shipped Rust literal that reads as a host path fails before any build")
    return 0


if __name__ == "__main__":
    sys.exit(main())
