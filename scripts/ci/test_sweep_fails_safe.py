#!/usr/bin/env python3
"""The sweeper must fail safe, not fail clean (#1634, codex).

Every case here is a FAILURE the script used to read as permission to delete:
`ps` unreadable meant "no build running", `git status` unreadable meant "clean",
and a failed rmtree still reported the full pre-scan size as freed. A sweeper
that reads failure as clean is worse than sweeping by hand, so each case proves
both directions: the failure refuses, the healthy equivalent still proceeds.
"""
from __future__ import annotations
import contextlib, importlib.util, io, os, pathlib, shutil, subprocess, sys, tempfile

HERE = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location(
    "sweeper", HERE / "sweep_merged_build_caches.py")
sweeper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sweeper)

fails: list[str] = []


def expect(what: str, ok: bool, detail: str = "") -> None:
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f"   [{detail}]" if not ok and detail else ""))
    if not ok:
        fails.append(what)


def with_fake_ps(body: str):
    """Put a fake `ps` first on PATH for the duration of one call."""
    d = tempfile.mkdtemp()
    p = pathlib.Path(d) / "ps"
    p.write_text(body)
    p.chmod(0o755)
    return d


print("the sweeper fails safe")

# --- ps: unknown must not authorise deletion -------------------------------
old_path = os.environ["PATH"]
for label, body, expect_busy in [
    ("ps exits non-zero", "#!/bin/sh\nexit 1\n", True),
    ("ps prints nothing", "#!/bin/sh\nexit 0\n", True),
    ("ps works and no build runs", "#!/bin/sh\necho '/bin/zsh'\necho '/usr/bin/vim'\n", False),
    ("ps works and rustc runs", "#!/bin/sh\necho '/usr/bin/rustc --edition 2021'\n", True),
]:
    d = with_fake_ps(body)
    os.environ["PATH"] = d + os.pathsep + old_path
    try:
        busy, why = sweeper.a_build_may_be_running()
    finally:
        os.environ["PATH"] = old_path
        shutil.rmtree(d, ignore_errors=True)
    expect(f"{label} -> {'refuses' if expect_busy else 'proceeds'}",
           busy is expect_busy, f"got busy={busy} why={why!r}")
    if expect_busy and "ps" in label and "no build" not in label and "rustc" not in label:
        expect("  and says the state is unknown, not that it is clean",
               "unknown" in why, why)

# --- run(): a failed git command is not an empty answer --------------------
ok, out = sweeper.run("rev-parse", "--show-toplevel")
expect("run() reports success on a real command", ok is True)
ok, out = sweeper.run("cat-file", "-e", "0000000000000000000000000000000000000000")
expect("run() reports FAILURE rather than an empty string", ok is False)

# --- git status unreadable: keep the worktree ------------------------------
# Proven through the real script: a worktree whose index cannot be read must be
# kept, not counted as clean and reclaimable.
def repo_with_worktree(root: pathlib.Path) -> pathlib.Path:
    r = root / "r"
    r.mkdir()
    env = sweeper.clean_env()
    def g(*a, cwd=r):
        return subprocess.run(["git", *a], cwd=cwd, capture_output=True, text=True, env=env)
    g("init", "-q", "-b", "master")
    g("config", "user.email", "t@t"); g("config", "user.name", "t")
    (r / "f.txt").write_text("x\n")
    # target/ must be ignored, or the worktree is "dirty" for holding it and no
    # case below ever reaches the code it means to test.
    (r / ".gitignore").write_text("target/\n")
    g("add", "-A"); g("commit", "-qm", "base")
    (r / "target").mkdir()
    (r / "target" / "big.bin").write_text("y" * 4096)
    return r


with tempfile.TemporaryDirectory() as d:
    r = repo_with_worktree(pathlib.Path(d))
    res = subprocess.run([sys.executable, str(HERE / "sweep_merged_build_caches.py"),
                          "--base", "master"], cwd=r, capture_output=True, text=True,
                         env=sweeper.clean_env())
    # "reclaimable" alone also matches "nothing reclaimable" -- the first version
    # of this assertion passed on the negative message and proved nothing.
    expect("a clean merged worktree IS reported reclaimable",
           "GB reclaimable across" in res.stdout,
           res.stdout[-200:] + res.stderr[-200:])

    # Now make the index unreadable and re-run.
    idx = r / ".git" / "index"
    idx.write_bytes(b"not an index at all")
    res = subprocess.run([sys.executable, str(HERE / "sweep_merged_build_caches.py"),
                          "--base", "master", "--sweep"], cwd=r, capture_output=True,
                         text=True, env=sweeper.clean_env())
    expect("an unreadable index does NOT make a worktree reclaimable",
           "could not be read" in res.stdout or "not a readable worktree" in res.stdout,
           res.stdout[-240:])
    expect("  and the target survives", (r / "target" / "big.bin").exists())

# --- rmtree failure: report it, and count only what went away --------------
# The first version of this case made the removal fail with chmod. As root, or
# with CAP_DAC_OVERRIDE -- which many CI containers run with -- rmtree ignores
# the mode, the target is deleted, both assertions fail and the cleanup raises
# FileNotFoundError on a path that is already gone. Codex reproduced it as root.
# A fixture that fails-as-intended only on one machine measures something else
# everywhere else, so the failure is injected directly and depends on no
# filesystem permission at all. (codex, #1641)
with tempfile.TemporaryDirectory() as d:
    r = repo_with_worktree(pathlib.Path(d))
    (r / "target" / "more.bin").write_text("z" * 2048)

    real_rmtree = sweeper.shutil.rmtree
    real_busy = sweeper.a_build_may_be_running
    old_cwd = os.getcwd()

    def refuses_to_delete(path, *a, **k):
        raise PermissionError(13, "Permission denied", str(path))

    buf_out, buf_err = io.StringIO(), io.StringIO()
    try:
        os.chdir(r)
        sweeper.shutil.rmtree = refuses_to_delete
        # Whether a build is running is covered by its own four cases above; here
        # it must simply not decide the outcome, so it is pinned to "no build".
        sweeper.a_build_may_be_running = lambda: (False, "")
        with contextlib.redirect_stdout(buf_out), contextlib.redirect_stderr(buf_err):
            rc = sweeper.main(["sweep", "--base", "master", "--sweep"])
    finally:
        sweeper.shutil.rmtree = real_rmtree
        sweeper.a_build_may_be_running = real_busy
        os.chdir(old_cwd)

    out, err = buf_out.getvalue(), buf_err.getvalue()
    expect("a failed removal exits non-zero", rc == 1, f"exit={rc} {out[-160:]}")
    expect("  and says the removal FAILED", "FAILED to remove" in err, err[-200:])
    expect("  and reports 0.0 GB freed, not the pre-scan size",
           "0.0 GB freed" in out, out[-200:])
    expect("  and the target is still there", (r / "target" / "more.bin").exists())

MINIMUM_CASES = 13  # FLOOR
print(f"\n  {len(fails)} failure(s)")
for f in fails:
    print(f"    - {f}")
raise SystemExit(1 if fails else 0)
