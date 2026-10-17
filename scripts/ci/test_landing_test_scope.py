#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The landing skips only the script tests a change cannot reach, and the shell obeys."""
import pathlib
import subprocess
import tempfile
import unittest

import landing_test_scope as scope
from fixture_env import sealed_env

ROOT = pathlib.Path(__file__).resolve().parents[2]

GATE = """\
run atest  python3 scripts/ci/test_a.py
zwaar btest bash scripts/ci/tests/b.sh
run ctest  python3 scripts/ci/test_c.py
run guard  python3 scripts/ci/guard.py
"""
FILES = {
    "scripts/ci/local_ci_gate.sh": GATE,
    "scripts/ci/test_a.py": "import a\n",
    "scripts/ci/a.py": "print('a')\n",
    "scripts/ci/tests/b.sh": "bash scripts/x/b_tool.sh\n",
    "scripts/x/b_tool.sh": "echo b\n",
    # Names c_tool.py only in a comment and a docstring, and runs cargo.
    "scripts/ci/test_c.py": '"""Unlike scripts/x/c_tool.py."""\n# c_tool.py\nimport subprocess\n'
                            'subprocess.run(["cargo", "test"])\n',
    "scripts/x/c_tool.py": "print('c')\n",
    "scripts/ci/guard.py": "print('guard')\n",
}


def fixture(extra=None):
    d = tempfile.TemporaryDirectory()
    root = pathlib.Path(d.name)
    for rel, text in {**FILES, **(extra or {})}.items():
        (root / rel).parent.mkdir(parents=True, exist_ok=True)
        (root / rel).write_text(text)
    return d, root


class ScopeTests(unittest.TestCase):
    def out(self, changed, extra=None):
        d, root = fixture(extra)
        with d:
            gate = (root / "scripts/ci/local_ci_gate.sh").read_text()
            return set(scope.out_of_landing(changed, gate, root))

    def test_only_tests_are_scoped(self):
        self.assertEqual(set(scope.test_gates(GATE)), {"atest", "btest", "ctest"})

    def test_a_test_runs_when_its_module_changes_and_not_otherwise(self):
        self.assertEqual(self.out({"scripts/ci/a.py"}), {"btest", "ctest"})
        self.assertEqual(self.out({"scripts/x/b_tool.sh"}), {"atest", "ctest"})

    def test_a_script_named_in_a_comment_is_not_an_input(self):
        self.assertIn("ctest", self.out({"scripts/x/c_tool.py"}))

    def test_a_test_that_runs_cargo_reaches_every_crate(self):
        self.assertEqual(self.out({"crates/pdf-engine/src/lib.rs"}), {"atest", "btest"})
        self.assertEqual(self.out({"Cargo.lock"}), {"atest", "btest"})

    def test_the_gate_the_scoper_the_hook_and_doubt_run_everything(self):
        for changed in ({"scripts/ci/local_ci_gate.sh"}, {"scripts/ci/landing_test_scope.py"},
                        {".githooks/pre-push"}, None):
            self.assertEqual(self.out(changed), set(), changed)

    def test_an_unreadable_input_runs_the_test(self):
        d, root = fixture()
        with d:
            (root / "scripts/ci/a.py").write_bytes(b"\xff\xfe\x00bad")
            gate = (root / "scripts/ci/local_ci_gate.sh").read_text()
            self.assertNotIn("atest", scope.out_of_landing({"docs/x.md"}, gate, root))

    def test_a_registry_is_an_input_but_not_a_way_through(self):
        many = "\n".join(f"scripts/x/tool_{i}.py" for i in range(scope.REGISTRY + 5)) + "\n"
        extra = {"scripts/ci/test_a.py": "import a\nimport registry\n", "scripts/ci/registry.py": f"X = '''\n'''\nL = [{many!r}]\n"}
        extra.update({f"scripts/x/tool_{i}.py": "" for i in range(scope.REGISTRY + 5)})
        self.assertIn("atest", self.out({"scripts/x/tool_3.py"}, extra))
        self.assertNotIn("atest", self.out({"scripts/ci/registry.py"}, extra))

    def test_the_real_gate_skips_the_infrastructure_tests_for_a_benchmark_script(self):
        # #2019 held the landing lock 68 minutes for two benchmark scripts.
        gate = (ROOT / scope.GATE).read_text()
        out = scope.out_of_landing({"scripts/ci/loop/structured_bench.py"}, gate, ROOT)
        for heavy in ("seedtest", "gatelock", "remotegate"):
            self.assertIn(heavy, out)
        # And a change to the remote gate still runs its own test.
        out = scope.out_of_landing({"scripts/cos/remote_gate.sh"}, gate, ROOT)
        self.assertNotIn("remotegate", out)

    def test_real_shell_skips_only_in_the_landing_lane(self):
        source = (ROOT / "scripts/ci/local_ci_gate.sh").read_text()
        run = source[source.index("run() {"):source.index("# The gates that cost minutes")]
        with tempfile.TemporaryDirectory() as directory:
            for landing, full, expected in ((1, 0, "0,0,1"), (1, 1, "1,0,0"), (0, 0, "1,0,0")):
                script = (f'LANDING={landing}; FULL={full}; LANDING_OUT_OF_REACH="seedtest"; '
                          f'DEVELOPMENT_DEFERRED=""; PLAATS=""; LOGDIR="{directory}"; fail=0; pass=0; '
                          'uitgesteld=0; ongeraakt=0; gevallen=""; hoort_hier() { return 0; }\n')
                script += run + "\nrun seedtest false\necho result=$fail,$uitgesteld,$ongeraakt\n"
                output = subprocess.check_output(["bash", "-c", script], text=True)
                self.assertIn(f"result={expected}", output, (landing, full))

    def test_it_is_wired_and_its_net_exists(self):
        local = (ROOT / "scripts/ci/local_ci_gate.sh").read_text()
        self.assertIn("python3 scripts/ci/landing_test_scope.py", local)
        self.assertIn("python3 scripts/ci/test_landing_test_scope.py", local)
        ci = (ROOT / ".github/workflows/ci.yml").read_text()
        self.assertIn("python3 scripts/ci/test_landing_test_scope.py", ci)
        nightly = ROOT / "scripts/cos/nightly_full_gate.sh"
        self.assertIn("remote_gate_lane.sh full", nightly.read_text())
        subprocess.check_call(["bash", "-n", str(nightly)])


class NightlyTests(unittest.TestCase):
    """The net keeps the COS's rules: the ordinary pool, never a second machine."""

    def run_nightly(self, busy=False, queued=False, elsewhere=False, nothing_new=False,
                    failed_before=False):
        with tempfile.TemporaryDirectory() as d:
            d = pathlib.Path(d)
            clone, pool, home = d / "clone", d / "pool", d / "home"
            (clone / "scripts/cos").mkdir(parents=True)
            pool.mkdir()
            nightly = (ROOT / "scripts/cos/nightly_full_gate.sh").read_text()
            (clone / "scripts/cos/nightly_full_gate.sh").write_text(nightly)
            (clone / "scripts/cos/remote_gate_lane.sh").write_text(
                f'echo "lane $1" >> "{d}/called"\n')
            # Sealed, and with a ledger of its own: the stand-in lane buys
            # nothing, but a fixture that names the remote gate never points at
            # the operator's book.
            env = sealed_env(identity=True, cwd=str(clone), PATH="/usr/bin:/bin", HOME=str(home),
                             NIGHTLY_HOME=str(home / "n"),
                             NIGHTLY_CLONE=str(d / "other" if elsewhere else clone),
                             NIGHTLY_POOL_DIR=str(pool), NIGHTLY_QUEUES=str(d / "queue"),
                             HETZNER_LEDGER=str(d / "ledger.jsonl"))
            subprocess.check_call(["git", "init", "-q", str(clone)], env=env)
            for message in ("before", "landed"):
                subprocess.check_call(["git", "-C", str(clone), "-c", "core.hooksPath=/dev/null",
                                       "commit", "-q", "--allow-empty", "-m", message], env=env)
            if nothing_new:
                (home / "n").mkdir(parents=True)
                (home / "n" / "last.sha").write_text(subprocess.check_output(
                    ["git", "-C", str(clone), "rev-parse", "HEAD"], env=env, text=True))
                if failed_before:
                    (home / "n" / "latest.status").write_text("FAIL abc 2026-09-28 log\n")
            (d / "other").mkdir()
            if busy:
                (pool / "gate-fast-x.busy.123").write_text("")
            if queued:
                (d / "queue").write_text("123 landing\n")
            rc = subprocess.run(["bash", str(clone / "scripts/cos/nightly_full_gate.sh")],
                                env=env, capture_output=True, text=True).returncode
            status = (home / "n" / "latest.status")
            return (rc, status.read_text().split()[0] if status.exists() else None,
                    (d / "called").read_text().strip() if (d / "called").exists() else None)

    def test_a_taken_pool_or_a_queue_skips_the_night_and_buys_nothing(self):
        self.assertEqual(self.run_nightly(busy=True), (0, "SKIP", None))
        self.assertEqual(self.run_nightly(queued=True), (0, "SKIP", None))

    def test_an_idle_pool_runs_the_full_lane(self):
        self.assertEqual(self.run_nightly(), (0, "PASS", "lane full"))

    def test_a_night_with_nothing_new_skips_and_keeps_a_failure(self):
        self.assertEqual(self.run_nightly(nothing_new=True), (0, "SKIP", None))
        self.assertEqual(self.run_nightly(nothing_new=True, failed_before=True), (0, "FAIL", None))

    def test_the_night_judges_what_landed_since_the_last_one(self):
        # The gate's message range is github/master..HEAD; on master it is
        # empty, and the message guard refused the first night (29-09).
        text = (ROOT / "scripts/cos/nightly_full_gate.sh").read_text()
        self.assertIn("update-ref refs/remotes/github/master", text)

    def test_it_runs_only_from_its_own_clone(self):
        self.assertEqual(self.run_nightly(elsewhere=True)[0], 2)

    def test_the_schedule_is_three_in_the_morning_on_arm64_paths(self):
        import plistlib
        raw = (ROOT / "scripts/cos/launchd/com.pdfluent.nightly-full-gate.plist").read_bytes()
        plist = plistlib.loads(raw)
        self.assertEqual(plist["StartCalendarInterval"], {"Hour": 3, "Minute": 0})
        path = plist["EnvironmentVariables"]["PATH"].split(":")
        self.assertEqual(path[0], "/usr/bin")
        self.assertEqual(path[-1], "/usr/local/bin")


if __name__ == "__main__":
    unittest.main()
