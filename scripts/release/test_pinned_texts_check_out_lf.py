#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The release texts pinned to tree bytes check out LF on every OS.

`fetch_models_release.py` pins four models-v1 release assets to the size and
sha256 of tree files (its TREE_TEXTS). The release carries the LF bytes. Git
for Windows ships `core.autocrlf=true` in its system config, so without an
attribute the win-x64 release smoke checked the texts out CRLF and refused a
correct release: "LICENSE is 34523 bytes, pin says 35184" (661 lines, one CR
each). The fix is `text eol=lf` in .gitattributes; the pins stay strict.

The cases:

  * every TREE_TEXTS path carries eol=lf, read off `git check-attr`, and its
    committed blob is LF (`git ls-files --eol`): the blob is the release text;
  * the CRLF proof, both ways, offline. A throwaway repository gets this
    tree's .gitattributes, the four committed texts and the fetch script with
    its pin tables; it is cloned with `core.autocrlf=true` -- the Windows
    runner's setting -- and the clone's own fetch_models_release.py runs its
    tree-pin path against the committed bytes as release assets (`--base
    file://`, the script's offline seam). With the attribute it stages all
    four; with the attribute lines removed (in the throwaway repository only)
    the clone checks out CRLF and the same run refuses.

The list comes from fetch_models_release.TREE_TEXTS itself, so a fifth pinned
text fails here until .gitattributes covers it.
"""
from __future__ import annotations

import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import fetch_models_release  # noqa: E402

REPO = fetch_models_release.REPO
FETCH = HERE / "fetch_models_release.py"
# The files the fetch script reads besides the texts: load_pins needs both
# tables to exist and parse before it pins anything.
SUPPORT = [FETCH, fetch_models_release.DEFAULT_MODELS_PINS,
           fetch_models_release.DEFAULT_ORT_PINS]

MINIMUM_CASES = 4  # FLOOR
GIT = shutil.which("git")


def _rel(path: pathlib.Path) -> str:
    return path.resolve().relative_to(REPO).as_posix()


PINNED = {asset: _rel(path) for asset, path in fetch_models_release.TREE_TEXTS.items()}


def _host_env() -> dict[str, str]:
    """Git env for this repository, without a hook's GIT_DIR/GIT_WORK_TREE."""
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def _sandbox_env() -> dict[str, str]:
    """Git env for the throwaway repositories: no system or global config.

    The proof must not depend on the host's config -- not a Windows system
    autocrlf, not a global hooksPath or signing rule. The only autocrlf in
    play is the one each clone sets explicitly.
    """
    env = _host_env()
    env["GIT_CONFIG_NOSYSTEM"] = "1"
    env["GIT_CONFIG_GLOBAL"] = os.devnull
    # The default global attributes file ($XDG_CONFIG_HOME/git/attributes,
    # else ~/.config/git/attributes) is read even without a global config.
    # Point both at a directory that does not exist, so it is never found.
    nohome = os.path.join(tempfile.gettempdir(), f"pinned-texts-no-home-{os.getpid()}")
    env["HOME"] = env["XDG_CONFIG_HOME"] = nohome
    env["GIT_AUTHOR_NAME"] = env["GIT_COMMITTER_NAME"] = "pinned-texts-test"
    env["GIT_AUTHOR_EMAIL"] = env["GIT_COMMITTER_EMAIL"] = "pinned-texts-test@invalid"
    return env


def _git(*args: str, cwd: pathlib.Path = REPO, env: dict[str, str] | None = None,
         binary: bool = False) -> str | bytes:
    proc = subprocess.run(
        [GIT or "git", *args], cwd=str(cwd), capture_output=True,
        text=not binary, check=False, env=env if env is not None else _host_env(),
        timeout=120,
    )
    if proc.returncode != 0:
        err = proc.stderr if isinstance(proc.stderr, str) else proc.stderr.decode(errors="replace")
        raise AssertionError(f"git {' '.join(args)} failed in {cwd}: {err.strip()[-400:]}")
    return proc.stdout


def _committed(rel: str) -> bytes:
    """The staged blob of a tree file: the LF bytes the release carries."""
    return _git("cat-file", "blob", f":{rel}", binary=True)  # type: ignore[return-value]


def _without_pinned_lines(attributes: str) -> tuple[str, list[str]]:
    """The attributes text with every line naming a pinned path removed."""
    kept, removed = [], []
    targets = set(PINNED.values())
    for line in attributes.splitlines(keepends=True):
        fields = line.split()
        if fields and not fields[0].startswith("#") and fields[0].lstrip("/") in targets:
            removed.append(line.rstrip("\n"))
        else:
            kept.append(line)
    return "".join(kept), removed


if GIT is None:
    print("SKIPPED (not a pass): git is not on PATH; the pinned-text eol "
          "attributes cannot be read", file=sys.stderr)


@unittest.skipIf(GIT is None, "git is not on PATH")
class PinnedTextsCheckOutLf(unittest.TestCase):
    def test_the_pin_list_is_the_four_root_texts(self) -> None:
        # A floor on the input: an empty TREE_TEXTS would pass every case below.
        self.assertEqual(
            sorted(PINNED.values()),
            sorted(["LICENSE", "LICENSE-APACHE", "LICENSE-COMMERCIAL",
                    "scripts/release/models-v1/MODELS-NOTICE.txt"]),
        )

    def test_every_pinned_text_carries_eol_lf(self) -> None:
        rels = sorted(PINNED.values())
        out = _git("check-attr", "text", "eol", "--", *rels)
        attrs: dict[str, dict[str, str]] = {}
        for line in str(out).splitlines():
            head, _, value = line.rpartition(": ")
            path, _, attr = head.rpartition(": ")
            attrs.setdefault(path.strip('"'), {})[attr] = value
        for rel in rels:
            with self.subTest(path=rel):
                seen = attrs.get(rel, {})
                self.assertEqual(
                    seen.get("eol"), "lf",
                    f"{rel}: eol={seen.get('eol')} text={seen.get('text')} -- "
                    "fetch_models_release.py pins this file's bytes; it needs "
                    "`text eol=lf` in .gitattributes or a Windows checkout "
                    "turns LF into CRLF and refuses the release",
                )
                self.assertEqual(seen.get("text"), "set", f"{rel}: want `text`")

    def test_every_pinned_blob_is_lf(self) -> None:
        out = _git("ls-files", "--eol", "--", *sorted(PINNED.values()))
        lines = [line for line in str(out).splitlines() if line]
        self.assertEqual(len(lines), len(PINNED), out)
        for line in lines:
            with self.subTest(line=line):
                self.assertTrue(line.startswith("i/lf "), line)

    def _origin(self, root: pathlib.Path, attributes: str) -> pathlib.Path:
        origin = root / "origin"
        env = _sandbox_env()
        _git("init", "-q", str(origin), cwd=root, env=env)
        (origin / ".gitattributes").write_text(attributes, encoding="utf-8", newline="")
        for rel in PINNED.values():
            dest = origin / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(_committed(rel))
        for path in SUPPORT:
            dest = origin / _rel(path)
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, dest)
        _git("add", ".", cwd=origin, env=env)
        _git("-c", "core.hooksPath=" + os.devnull, "-c", "commit.gpgsign=false",
             "commit", "-q", "-m", "fixture", cwd=origin, env=env)
        return origin

    def _clone_and_fetch(self, root: pathlib.Path, origin: pathlib.Path,
                         name: str) -> tuple[pathlib.Path, subprocess.CompletedProcess]:
        clone = root / name
        _git("clone", "-q", "--no-local", "--config", "core.autocrlf=true",
             str(origin), str(clone), cwd=root, env=_sandbox_env())
        assets = root / f"{name}-release"
        assets.mkdir()
        for asset, rel in PINNED.items():
            (assets / asset).write_bytes(_committed(rel))
        flags: list[str] = []
        for asset in sorted(PINNED):
            flags += ["--asset", asset]
        proc = subprocess.run(
            [sys.executable, str(clone / _rel(FETCH)), "--out", str(root / f"{name}-out"),
             *flags, "--base", assets.as_uri()],
            capture_output=True, text=True, timeout=120, env=_host_env(),
        )
        return clone, proc

    def test_a_crlf_checkout_stages_with_the_attribute_and_refuses_without(self) -> None:
        attributes = (REPO / ".gitattributes").read_text(encoding="utf-8")
        stripped, removed = _without_pinned_lines(attributes)
        with tempfile.TemporaryDirectory(prefix="pinned-texts-lf-") as tmp:
            root = pathlib.Path(tmp)

            # With this tree's attributes: LF on an autocrlf checkout, pins hold.
            (root / "a").mkdir()
            with_attr = self._origin(root / "a", attributes)
            clone, proc = self._clone_and_fetch(root / "a", with_attr, "clone")
            for rel in PINNED.values():
                with self.subTest(case="with attribute", path=rel):
                    self.assertFalse(b"\r\n" in (clone / rel).read_bytes(),
                                     f"{rel} checked out CRLF under core.autocrlf=true")
            self.assertEqual(proc.returncode, 0,
                             "the fetch refused a correct release on an autocrlf "
                             f"checkout:\n{proc.stdout}{proc.stderr}")
            for asset in PINNED:
                self.assertIn(f"ok {asset}", proc.stdout)

            # Without them: the same clone path goes CRLF and the pins refuse.
            # This half proves the first half measures the attribute, not luck.
            self.assertEqual(
                len(removed), len(PINNED),
                f"expected one .gitattributes line per pinned text, found {removed}",
            )
            (root / "b").mkdir()
            bare = self._origin(root / "b", stripped)
            clone, proc = self._clone_and_fetch(root / "b", bare, "clone")
            for rel in PINNED.values():
                with self.subTest(case="without attribute", path=rel):
                    self.assertTrue(b"\r\n" in (clone / rel).read_bytes(),
                                  f"{rel} stayed LF without its attribute: the "
                                  "sandbox did not apply core.autocrlf=true")
            self.assertEqual(proc.returncode, 1, proc.stdout + proc.stderr)
            self.assertIn("REFUSED", proc.stderr)
            self.assertIn("pin says", proc.stderr)


if __name__ == "__main__":
    cases = unittest.TestLoader().loadTestsFromTestCase(PinnedTextsCheckOutLf).countTestCases()
    if cases < MINIMUM_CASES:  # FLOOR
        print(
            f"[test-pinned-texts-check-out-lf] FATAL: {cases} cases, floor is "
            f"{MINIMUM_CASES}. A suite that lost its cases must not report green.",
            file=sys.stderr,
        )
        sys.exit(1)
    unittest.main(verbosity=2)
